use crate::annis::db::aql;
use crate::annis::db::aql::operators;
use crate::annis::db::aql::operators::RangeSpec;
use crate::annis::db::exec::nodesearch::NodeSearchSpec;
use crate::annis::db::plan::ExecutionPlan;
use crate::annis::db::query;
use crate::annis::db::query::conjunction::Conjunction;
use crate::annis::db::query::disjunction::Disjunction;
use crate::annis::db::relannis;
use crate::annis::db::sort_matches::CollationType;
use crate::annis::db::token_helper;
use crate::annis::db::token_helper::TokenHelper;
use crate::annis::errors::*;
use crate::annis::types::CountExtra;
use crate::annis::types::{
    CorpusConfiguration, FrequencyTable, FrequencyTableRow, QueryAttributeDescription,
};
use crate::annis::util::quicksort;
use crate::annis::{db, util::TimeoutCheck};
use crate::{
    graph::Match,
    malloc_size_of::{MallocSizeOf, MallocSizeOfOps},
    AnnotationGraph,
};
use fmt::Display;
use fs2::FileExt;
use graphannis_core::{
    annostorage::{MatchGroup, ValueSearch},
    graph::{
        storage::GraphStatistic, update::GraphUpdate, ANNIS_NS, NODE_NAME, NODE_NAME_KEY, NODE_TYPE,
    },
    types::{AnnoKey, Annotation, Component, Edge, NodeID},
    util::memory_estimation,
};
use linked_hash_map::LinkedHashMap;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use smartstring::alias::String as SmartString;
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Condvar, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::{borrow::Cow, time::Duration};

use rustc_hash::FxHashMap;

use rand::seq::SliceRandom;
use std::{
    ffi::CString,
    io::{BufReader, Write},
};

use aql::model::AnnotationComponentType;
use db::AnnotationStorage;

#[cfg(test)]
mod tests;

const MAX_VECTOR_RESERVATION: usize = 10_000_000;

enum CacheEntry {
    Loaded(AnnotationGraph),
    NotLoaded,
}

/// Indicates if the corpus is partially or fully loaded into the main memory cache.
#[derive(Debug, Ord, Eq, PartialOrd, PartialEq)]
pub enum LoadStatus {
    /// Corpus is not loaded into main memory at all.
    NotLoaded,
    /// Corpus is partially loaded and is estimated to use the given amount of main memory in bytes.
    /// Partially means that the node annotations are and optionally some graph storages are loaded.
    PartiallyLoaded(usize),
    /// Corpus is fully loaded (node annotation information and all graph storages) and is estimated to use the given amount of main memory in bytes.
    FullyLoaded(usize),
}

/// Information about a single graph storage of the corpus.
pub struct GraphStorageInfo {
    /// The component this graph storage belongs to.
    pub component: Component<AnnotationComponentType>,
    /// Indicates if the graph storage is loaded or not.
    pub load_status: LoadStatus,
    /// Number of edge annotations in this graph storage.
    pub number_of_annotations: usize,
    /// Name of the implementation
    pub implementation: String,
    /// Graph statistics
    pub statistics: Option<GraphStatistic>,
}

impl fmt::Display for GraphStorageInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "Component {}: {} annnotations",
            self.component, self.number_of_annotations
        )?;
        if let Some(stats) = &self.statistics {
            writeln!(f, "Stats: {}", stats)?;
        }
        writeln!(f, "Implementation: {}", self.implementation)?;
        match self.load_status {
            LoadStatus::NotLoaded => writeln!(f, "Not Loaded")?,
            LoadStatus::PartiallyLoaded(memory_size) => {
                writeln!(f, "Status: {:?}", "partially loaded")?;
                writeln!(
                    f,
                    "Memory: {:.2} MB",
                    memory_size as f64 / f64::from(1024 * 1024)
                )?;
            }
            LoadStatus::FullyLoaded(memory_size) => {
                writeln!(f, "Status: {:?}", "fully loaded")?;
                writeln!(
                    f,
                    "Memory: {:.2} MB",
                    memory_size as f64 / f64::from(1024 * 1024)
                )?;
            }
        };
        Ok(())
    }
}

/// Information about a corpus that is part of the corpus storage.
pub struct CorpusInfo {
    /// Name of the corpus.
    pub name: String,
    /// Indicates if the corpus is partially or fully loaded.
    pub load_status: LoadStatus,
    /// The amount of memory that the node annotations are using
    pub node_annos_load_size: Option<usize>,
    /// A list of descriptions for the graph storages of this corpus.
    pub graphstorages: Vec<GraphStorageInfo>,
    /// The current configuration of this corpus.
    /// This information is stored in the "corpus-config.toml` file in the data directory
    /// and loaded on demand.
    pub config: CorpusConfiguration,
}

impl fmt::Display for CorpusInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.load_status {
            LoadStatus::NotLoaded => writeln!(f, "Not Loaded")?,
            LoadStatus::PartiallyLoaded(memory_size) => {
                writeln!(f, "Status: {:?}", "partially loaded")?;
                writeln!(
                    f,
                    "Total memory: {:.2} MB",
                    memory_size as f64 / f64::from(1024 * 1024)
                )?;
            }
            LoadStatus::FullyLoaded(memory_size) => {
                writeln!(f, "Status: {:?}", "fully loaded")?;
                writeln!(
                    f,
                    "Total memory: {:.2} MB",
                    memory_size as f64 / f64::from(1024 * 1024)
                )?;
            }
        };
        if let Some(memory_size) = self.node_annos_load_size {
            writeln!(
                f,
                "Node Annotations: {:.2} MB",
                memory_size as f64 / f64::from(1024 * 1024)
            )?;
        }
        if !self.graphstorages.is_empty() {
            writeln!(f, "------------")?;
            for gs in &self.graphstorages {
                write!(f, "{}", gs)?;
                writeln!(f, "------------")?;
            }
        }
        Ok(())
    }
}

/// Defines the order of results of a `find` query.
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub enum ResultOrder {
    /// Order results by their document name and the the text position of the match.
    Normal,
    /// Inverted the order of `Normal`.
    Inverted,
    /// A random ordering which is **not stable**. Each new query will result in a different order.
    Randomized,
    /// Results are not ordered at all, but also not actively randomized
    /// Each new query *might* result in a different order.
    NotSorted,
}

impl Default for ResultOrder {
    fn default() -> Self {
        ResultOrder::Normal
    }
}

struct PreparationResult<'a> {
    query: Disjunction<'a>,
    db_entry: Arc<RwLock<CacheEntry>>,
}

/// Definition of a single attribute of a frequency query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyDefEntry {
    /// The namespace of the annotation from which the attribute value is generated.
    #[serde(default)]
    pub ns: Option<String>,
    /// The name of the annotation from which the attribute value is generated.
    pub name: String,
    /// The name of the query node from which the attribute value is generated.
    pub node_ref: String,
}

impl FromStr for FrequencyDefEntry {
    type Err = GraphAnnisError;
    fn from_str(s: &str) -> std::result::Result<FrequencyDefEntry, Self::Err> {
        let splitted: Vec<&str> = s.splitn(2, ':').collect();
        if splitted.len() != 2 {
            return Err(GraphAnnisError::InvalidFrequencyDefinition);
        }
        let node_ref = splitted[0];
        let anno_key = graphannis_core::util::split_qname(splitted[1]);

        Ok(FrequencyDefEntry {
            ns: anno_key.0.map(String::from),
            name: String::from(anno_key.1),
            node_ref: String::from(node_ref),
        })
    }
}

/// An enum over all supported query languages of graphANNIS.
///
/// Currently, only the ANNIS Query Language (AQL) and its variants are supported, but this enum allows us to add a support for older query language versions
/// or completely new query languages.
#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum QueryLanguage {
    AQL,
    /// Emulates the (sometimes problematic) behavior of AQL used in ANNIS 3
    AQLQuirksV3,
}

impl Default for QueryLanguage {
    fn default() -> Self {
        QueryLanguage::AQL
    }
}

/// An enum of all supported input formats of graphANNIS.
#[repr(C)]
#[derive(Clone, Copy)]
pub enum ImportFormat {
    /// Legacy [relANNIS import file format](http://korpling.github.io/ANNIS/4.0/developer-guide/annisimportformat.html)
    RelANNIS,
    /// [GraphML](http://graphml.graphdrawing.org/) based export-format, suitable to be imported from other graph databases.
    /// This format follows the extensions/conventions of the Neo4j [GraphML module](https://neo4j.com/docs/labs/apoc/current/import/graphml/).
    GraphML,
}

/// An enum of all supported output formats of graphANNIS.
#[repr(C)]
#[derive(Clone, Copy)]
pub enum ExportFormat {
    /// [GraphML](http://graphml.graphdrawing.org/) based export-format, suitable to be imported into other graph databases.
    /// This format follows the extensions/conventions of the Neo4j [GraphML module](https://neo4j.com/docs/labs/apoc/current/import/graphml/).
    GraphML,
    /// Like `GraphML`, but compressed as ZIP file. Linked files are also copied into the ZIP file.
    GraphMLZip,
    /// Like `GraphML`, but using a directory with multiple GraphML files, each for one corpus.
    GraphMLDirectory,
}

/// Different strategies how it is decided when corpora need to be removed from the cache.
#[derive(Debug, Deserialize, Clone)]
pub enum CacheStrategy {
    /// Fixed maximum size of the cache in Megabytes.
    /// Before and after a new entry is loaded, the cache is cleared to have at maximum this given size.
    /// The loaded entry is always added to the cache, even if the single corpus is larger than the maximum size.
    FixedMaxMemory(usize),
    /// Maximum percent of the current free space/memory available.
    /// E.g. if the percent is 25 and there is 4,5 GB of free memory not used by the cache itself, the cache will use at most 1,125 GB memory.
    /// Cache size is checked before and after a corpus is loaded.
    /// The loaded entry is always added to the cache, even if the single corpus is larger than the maximum size.
    PercentOfFreeMemory(f64),
}

impl Display for CacheStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheStrategy::FixedMaxMemory(megabytes) => write!(f, "{} MB", megabytes),
            CacheStrategy::PercentOfFreeMemory(percent) => write!(f, "{}%", percent),
        }
    }
}

impl Default for CacheStrategy {
    fn default() -> Self {
        CacheStrategy::PercentOfFreeMemory(25.0)
    }
}

pub const SALT_URI_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b':').add(b'%');
const QUIRKS_SALT_URI_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'%');
pub const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    .add(b'%')
    .add(b'/');

/// Common arguments to all search queries.
#[derive(Debug, Clone)]
pub struct SearchQuery<'a, S: AsRef<str>> {
    /// The name of the corpora to execute the query on.
    pub corpus_names: &'a [S],
    /// The query as string.
    pub query: &'a str,
    ///  The query language of the query (e.g. AQL).
    pub query_language: QueryLanguage,
    /// If not `None`, the query will be aborted after running for the given amount of time.
    pub timeout: Option<Duration>,
}

/// A thread-safe API for managing corpora stored in a common location on the file system.
///
/// Multiple corpora can be part of a corpus storage and they are identified by their unique name.
/// Corpora are loaded from disk into main memory on demand:
/// An internal main memory cache is used to avoid re-loading a recently queried corpus from disk again.
pub struct CorpusStorage {
    db_dir: PathBuf,
    lock_file: File,
    cache_strategy: CacheStrategy,
    corpus_cache: RwLock<LinkedHashMap<String, Arc<RwLock<CacheEntry>>>>,
    query_config: query::Config,
    active_background_workers: Arc<(Mutex<usize>, Condvar)>,
}

fn init_locale() {
    // use collation as defined by the environment variables (LANGUAGE, LC_*, etc.)
    unsafe {
        let locale = CString::new("").unwrap_or_default();
        libc::setlocale(libc::LC_COLLATE, locale.as_ptr());
    }
}

fn add_subgraph_precedence(
    query: &mut Disjunction,
    ctx: usize,
    m: &NodeSearchSpec,
    left: bool,
) -> Result<()> {
    // nodes overlapping tokens left/right of match (using reflexive overlap to include the token itself):
    // node _o_ tok .0,ctx m
    // m .0,ctx tok _o_ node
    {
        let mut q = Conjunction::new();

        let node_idx = q.add_node(NodeSearchSpec::AnyNode, None);
        let tok_idx = q.add_node(NodeSearchSpec::AnyToken, None);
        let m_idx = q.add_node(m.clone(), None);
        q.add_operator(
            Box::new(operators::OverlapSpec { reflexive: true }),
            &node_idx,
            &tok_idx,
            true,
        )?;
        q.add_operator(
            Box::new(operators::PrecedenceSpec {
                segmentation: None,
                dist: RangeSpec::Bound {
                    min_dist: 0,
                    max_dist: ctx,
                },
            }),
            if left { &tok_idx } else { &m_idx },
            if left { &m_idx } else { &tok_idx },
            true,
        )?;
        query.alternatives.push(q);
    }

    Ok(())
}

fn add_subgraph_precedence_with_segmentation(
    query: &mut Disjunction,
    ctx: usize,
    segmentation: &str,
    m: &NodeSearchSpec,
    left: bool,
) -> Result<()> {
    // nodes overlapping the ones directly left/right of match (using reflexive overlap):
    // target _o_ node .seg,0,ctx m_node _o_ m
    // m _o_ m_node .0.ctx node  _o_ target
    {
        let mut q = Conjunction::new();
        // Since only the first node is included in the result, make sure the target node is the first node of th query
        let target_idx = q.add_node(NodeSearchSpec::AnyNode, None);
        let node_idx = q.add_node(NodeSearchSpec::AnyNode, None);
        let m_node_idx = q.add_node(NodeSearchSpec::AnyNode, None);
        let m_idx = q.add_node(m.clone(), None);

        q.add_operator(
            Box::new(operators::OverlapSpec { reflexive: true }),
            &m_node_idx,
            &m_idx,
            false,
        )?;

        q.add_operator(
            Box::new(operators::OverlapSpec { reflexive: true }),
            &target_idx,
            &node_idx,
            false,
        )?;

        q.add_operator(
            Box::new(operators::PrecedenceSpec {
                segmentation: Some(segmentation.to_string()),
                dist: RangeSpec::Bound {
                    min_dist: 0,
                    max_dist: ctx,
                },
            }),
            if left { &node_idx } else { &m_node_idx },
            if left { &m_node_idx } else { &node_idx },
            false,
        )?;
        query.alternatives.push(q);
    }

    Ok(())
}

/// Creates a new vector with the capacity to hold the expected number of items, but make sure the
/// capacity is memory aligned with the page size (only full pages are allocated).
fn new_vector_with_memory_aligned_capacity<T>(expected_len: usize) -> Vec<T> {
    let page_size = page_size::get();
    // Make sure the capacity is a multiple of the page size to avoid memory fragmentation
    let expected_memory_size = std::mem::size_of::<T>() * expected_len;
    let aligned_memory_size =
        expected_memory_size + (page_size - (expected_memory_size % page_size));

    Vec::with_capacity(aligned_memory_size / std::mem::size_of::<T>())
}

type FindIterator<'a> = Box<dyn Iterator<Item = MatchGroup> + 'a>;

impl CorpusStorage {
    /// Create a new instance with a maximum size for the internal corpus cache.
    ///
    /// - `db_dir` - The path on the filesystem where the corpus storage content is located. Must be an existing directory.
    /// - `cache_strategy`: A strategy for clearing the cache.
    /// - `use_parallel_joins` - If `true` parallel joins are used by the system, using all available cores.
    pub fn with_cache_strategy(
        db_dir: &Path,
        cache_strategy: CacheStrategy,
        use_parallel_joins: bool,
    ) -> Result<CorpusStorage> {
        init_locale();

        let query_config = query::Config { use_parallel_joins };

        #[allow(clippy::mutex_atomic)]
        let active_background_workers = Arc::new((Mutex::new(0), Condvar::new()));
        let cs = CorpusStorage {
            db_dir: PathBuf::from(db_dir),
            lock_file: create_lockfile_for_directory(db_dir)?,
            cache_strategy,
            corpus_cache: RwLock::new(LinkedHashMap::new()),
            query_config,
            active_background_workers,
        };

        Ok(cs)
    }

    /// Create a new instance with a an automatic determined size of the internal corpus cache.
    ///
    /// Currently, set the maximum cache size to 25% of the available/free memory at construction time.
    /// This behavior can change in the future.
    ///
    /// - `db_dir` - The path on the filesystem where the corpus storage content is located. Must be an existing directory.
    /// - `use_parallel_joins` - If `true` parallel joins are used by the system, using all available cores.
    pub fn with_auto_cache_size(db_dir: &Path, use_parallel_joins: bool) -> Result<CorpusStorage> {
        init_locale();

        let query_config = query::Config { use_parallel_joins };

        // get the amount of available memory, use a quarter of it per default
        let cache_strategy: CacheStrategy = CacheStrategy::PercentOfFreeMemory(25.0);

        #[allow(clippy::mutex_atomic)]
        let active_background_workers = Arc::new((Mutex::new(0), Condvar::new()));

        let cs = CorpusStorage {
            db_dir: PathBuf::from(db_dir),
            lock_file: create_lockfile_for_directory(db_dir)?,
            cache_strategy,
            corpus_cache: RwLock::new(LinkedHashMap::new()),
            query_config,
            active_background_workers,
        };

        Ok(cs)
    }

    /// List  all available corpora in the corpus storage.
    pub fn list(&self) -> Result<Vec<CorpusInfo>> {
        let names: Vec<String> = self.list_from_disk().unwrap_or_default();
        let mut result: Vec<CorpusInfo> = vec![];

        let mut mem_ops =
            MallocSizeOfOps::new(memory_estimation::platform::usable_size, None, None);

        for n in names {
            let corpus_info = self.create_corpus_info(&n, &mut mem_ops)?;
            result.push(corpus_info);
        }

        Ok(result)
    }

    fn list_from_disk(&self) -> Result<Vec<String>> {
        let mut corpora: Vec<String> = Vec::new();
        let directories =
            self.db_dir
                .read_dir()
                .map_err(|e| CorpusStorageError::ListingDirectories {
                    source: e,
                    path: self.db_dir.to_string_lossy().to_string(),
                })?;
        for c_dir in directories {
            let c_dir = c_dir.map_err(|e| CorpusStorageError::DirectoryEntry {
                source: e,
                path: self.db_dir.to_string_lossy().to_string(),
            })?;
            let ftype = c_dir
                .file_type()
                .map_err(|e| CorpusStorageError::FileTypeDetection {
                    source: e,
                    path: self.db_dir.to_string_lossy().to_string(),
                })?;
            if ftype.is_dir() {
                let directory_name = c_dir.file_name();
                let corpus_name = directory_name.to_string_lossy();
                // Use the decoded corpus name instead of the directory name
                let corpus_name = percent_decode_str(&corpus_name);
                corpora.push(corpus_name.decode_utf8_lossy().to_string());
            }
        }
        Ok(corpora)
    }

    fn get_corpus_config(&self, corpus_name: &str) -> Result<Option<CorpusConfiguration>> {
        let corpus_config_path = self.db_dir.join(corpus_name).join("corpus-config.toml");
        if corpus_config_path.is_file() {
            let file_content = std::fs::read_to_string(corpus_config_path)?;
            let config = toml::from_str(&file_content)?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    fn create_corpus_info(
        &self,
        corpus_name: &str,
        mem_ops: &mut MallocSizeOfOps,
    ) -> Result<CorpusInfo> {
        let cache_entry = self.get_entry(corpus_name)?;
        let lock = cache_entry.read().unwrap();

        // Read configuration file or create a default one
        let config: CorpusConfiguration = self
            .get_corpus_config(corpus_name)
            .map_err(|e| CorpusStorageError::LoadingCorpusConfig {
                corpus: corpus_name.to_string(),
                source: Box::new(e),
            })?
            .unwrap_or_default();

        let corpus_info: CorpusInfo = match &*lock {
            CacheEntry::Loaded(ref db) => {
                // check if all components are loaded
                let heap_size = db.size_of(mem_ops);
                let mut load_status = LoadStatus::FullyLoaded(heap_size);
                let node_annos_load_size = Some(db.get_node_annos().size_of(mem_ops));

                let mut graphstorages = Vec::new();
                for c in db.get_all_components(None, None) {
                    if let Some(gs) = db.get_graphstorage_as_ref(&c) {
                        graphstorages.push(GraphStorageInfo {
                            component: c.clone(),
                            load_status: LoadStatus::FullyLoaded(gs.size_of(mem_ops)),
                            number_of_annotations: gs.get_anno_storage().number_of_annotations(),
                            implementation: gs.serialization_id().clone(),
                            statistics: gs.get_statistics().cloned(),
                        });
                    } else {
                        load_status = LoadStatus::PartiallyLoaded(heap_size);
                        graphstorages.push(GraphStorageInfo {
                            component: c.clone(),
                            load_status: LoadStatus::NotLoaded,
                            number_of_annotations: 0,
                            implementation: "".to_owned(),
                            statistics: None,
                        })
                    }
                }

                CorpusInfo {
                    name: corpus_name.to_owned(),
                    load_status,
                    graphstorages,
                    node_annos_load_size,
                    config,
                }
            }
            &CacheEntry::NotLoaded => CorpusInfo {
                name: corpus_name.to_owned(),
                load_status: LoadStatus::NotLoaded,
                graphstorages: vec![],
                node_annos_load_size: None,
                config,
            },
        };
        Ok(corpus_info)
    }

    /// Return detailled information about a specific corpus with a given name (`corpus_name`).
    pub fn info(&self, corpus_name: &str) -> Result<CorpusInfo> {
        let mut mem_ops =
            MallocSizeOfOps::new(memory_estimation::platform::usable_size, None, None);
        self.create_corpus_info(corpus_name, &mut mem_ops)
    }

    fn get_entry(&self, corpus_name: &str) -> Result<Arc<RwLock<CacheEntry>>> {
        let corpus_name = corpus_name.to_string();

        {
            // test with read-only access if corpus is contained in cache
            let cache_lock = self.corpus_cache.read().unwrap();
            let cache = &*cache_lock;
            if let Some(e) = cache.get(&corpus_name) {
                return Ok(e.clone());
            }
        }

        // if not yet available, change to write-lock and insert cache entry
        let mut cache_lock = self.corpus_cache.write().unwrap();
        let cache = &mut *cache_lock;

        let entry = cache
            .entry(corpus_name)
            .or_insert_with(|| Arc::new(RwLock::new(CacheEntry::NotLoaded)));

        Ok(entry.clone())
    }

    fn load_entry_with_lock(
        &self,
        cache_lock: &mut RwLockWriteGuard<LinkedHashMap<String, Arc<RwLock<CacheEntry>>>>,
        corpus_name: &str,
        create_if_missing: bool,
    ) -> Result<Arc<RwLock<CacheEntry>>> {
        let cache = &mut *cache_lock;

        // if not loaded yet, get write-lock and load entry
        let escaped_corpus_name: Cow<str> =
            utf8_percent_encode(&corpus_name, PATH_SEGMENT_ENCODE_SET).into();
        let db_path: PathBuf = [self.db_dir.to_string_lossy().as_ref(), &escaped_corpus_name]
            .iter()
            .collect();

        let create_corpus = if db_path.is_dir() {
            false
        } else if create_if_missing {
            true
        } else {
            return Err(GraphAnnisError::NoSuchCorpus(corpus_name.to_string()));
        };

        // make sure the cache is not too large before adding the new corpus
        check_cache_size_and_remove_with_cache(cache, &self.cache_strategy, vec![], false);

        let db = if create_corpus {
            // create the default graph storages that are assumed to exist in every corpus
            let mut db = AnnotationGraph::with_default_graphstorages(false)?;

            // save corpus to the path where it should be stored
            db.persist_to(&db_path)
                .map_err(|e| CorpusStorageError::CreateCorpus {
                    corpus: corpus_name.to_string(),
                    source: e,
                })?;
            db
        } else {
            let mut db = AnnotationGraph::new(false)?;
            db.load_from(&db_path, false)?;
            db
        };

        let entry = Arc::new(RwLock::new(CacheEntry::Loaded(db)));
        // first remove entry, than add it: this ensures it is at the end of the linked hash map
        cache.remove(corpus_name);
        cache.insert(String::from(corpus_name), entry.clone());
        info!("Loaded corpus {}", corpus_name,);
        check_cache_size_and_remove_with_cache(
            cache,
            &self.cache_strategy,
            vec![corpus_name],
            true,
        );

        Ok(entry)
    }

    fn get_loaded_entry(
        &self,
        corpus_name: &str,
        create_if_missing: bool,
    ) -> Result<Arc<RwLock<CacheEntry>>> {
        let cache_entry = self.get_entry(corpus_name)?;

        // check if basics (node annotation, strings) of the database are loaded
        let loaded = {
            let lock = cache_entry.read().unwrap();
            matches!(&*lock, CacheEntry::Loaded(_))
        };

        if loaded {
            Ok(cache_entry)
        } else {
            let mut cache_lock = self.corpus_cache.write().unwrap();
            self.load_entry_with_lock(&mut cache_lock, corpus_name, create_if_missing)
        }
    }

    fn get_loaded_entry_with_components(
        &self,
        corpus_name: &str,
        components: Vec<Component<AnnotationComponentType>>,
    ) -> Result<Arc<RwLock<CacheEntry>>> {
        let db_entry = self.get_loaded_entry(corpus_name, false)?;
        let missing_components = {
            let lock = db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;

            let mut missing: HashSet<_> = HashSet::new();
            for c in components {
                if !db.is_loaded(&c) {
                    missing.insert(c);
                }
            }
            missing
        };
        if !missing_components.is_empty() {
            // load the needed components
            let mut lock = db_entry.write().unwrap();
            let db = get_write_or_error(&mut lock)?;
            for c in missing_components {
                db.ensure_loaded(&c)?;
            }
        };

        Ok(db_entry)
    }

    fn get_fully_loaded_entry(&self, corpus_name: &str) -> Result<Arc<RwLock<CacheEntry>>> {
        let db_entry = self.get_loaded_entry(corpus_name, false)?;
        let missing_components = {
            let lock = db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;

            let mut missing: HashSet<_> = HashSet::new();
            for c in db.get_all_components(None, None) {
                if !db.is_loaded(&c) {
                    missing.insert(c);
                }
            }
            missing
        };
        if !missing_components.is_empty() {
            // load the needed components
            let mut lock = db_entry.write().unwrap();
            let db = get_write_or_error(&mut lock)?;
            for c in missing_components {
                db.ensure_loaded(&c)?;
            }
        };

        Ok(db_entry)
    }

    /// Import all corpora from a ZIP file.
    ///
    /// This function will unzip the file to a temporary location and find all relANNIS and GraphML files in the ZIP file.
    /// The formats of the corpora can be relANNIS or GraphML.
    /// - `zip_file` - The content of the ZIP file.
    /// - `disk_based` - If `true`, prefer disk-based annotation and graph storages instead of memory-only ones.
    /// - `overwrite_existing` - If `true`, overwrite existing corpora. Otherwise ignore.
    /// - `progress_callback` - A callback function to which the import progress is reported to.
    ///
    /// Returns the names of the imported corpora.
    pub fn import_all_from_zip<R, F>(
        &self,
        zip_file: R,
        disk_based: bool,
        overwrite_existing: bool,
        progress_callback: F,
    ) -> Result<Vec<String>>
    where
        R: Read + Seek,
        F: Fn(&str),
    {
        // Unzip all files to a temporary directory
        let tmp_dir = tempfile::tempdir()?;
        debug!(
            "Using temporary directory {} to extract ZIP file content.",
            tmp_dir.path().to_string_lossy()
        );
        let mut archive = zip::ZipArchive::new(zip_file)?;

        let mut relannis_files = Vec::new();
        let mut graphannis_files = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let output_path = tmp_dir.path().join(file.sanitized_name());

            if let Some(file_name) = output_path.file_name() {
                if file_name == "corpus.annis" || file_name == "corpus.tab" {
                    if let Some(relannis_root) = output_path.parent() {
                        relannis_files.push(relannis_root.to_owned())
                    }
                } else if let Some(ext) = output_path.extension() {
                    if ext.to_string_lossy().to_ascii_lowercase() == "graphml" {
                        graphannis_files.push(output_path.clone());
                    }
                }
            }

            debug!(
                "copying ZIP file content {}",
                file.sanitized_name().to_string_lossy(),
            );
            if file.is_dir() {
                std::fs::create_dir_all(output_path)?;
            } else if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
                let mut output_file = std::fs::File::create(&output_path)?;
                std::io::copy(&mut file, &mut output_file)?;
            }
        }

        let mut corpus_names = Vec::new();

        // Import all relANNIS files
        for p in relannis_files {
            info!("importing relANNIS corpus from {}", p.to_string_lossy());
            let name = self.import_from_fs(
                &p,
                ImportFormat::RelANNIS,
                None,
                disk_based,
                overwrite_existing,
                &progress_callback,
            )?;
            corpus_names.push(name);
        }
        // Import all GraphML files
        for p in graphannis_files {
            info!("importing corpus from {}", p.to_string_lossy());
            let name = self.import_from_fs(
                &p,
                ImportFormat::GraphML,
                None,
                disk_based,
                overwrite_existing,
                &progress_callback,
            )?;
            corpus_names.push(name);
        }

        // Delete temporary directory
        debug!(
            "deleting temporary directory {}",
            tmp_dir.path().to_string_lossy()
        );
        std::fs::remove_dir_all(tmp_dir.path())?;

        Ok(corpus_names)
    }

    /// Import a corpus from an external location on the file system into this corpus storage.
    ///
    /// - `path` - The location on the file system where the corpus data is located.
    /// - `format` - The format in which this corpus data is stored.
    /// - `corpus_name` - Optionally override the name of the new corpus for file formats that already provide a corpus name. This only works if the imported file location only contains one corpus.
    /// - `disk_based` - If `true`, prefer disk-based annotation and graph storages instead of memory-only ones.
    /// - `overwrite_existing` - If `true`, overwrite existing corpora. Otherwise ignore.
    /// - `progress_callback` - A callback function to which the import progress is reported to.
    ///
    /// Returns the name of the imported corpus.
    pub fn import_from_fs<F>(
        &self,
        path: &Path,
        format: ImportFormat,
        corpus_name: Option<String>,
        disk_based: bool,
        overwrite_existing: bool,
        progress_callback: F,
    ) -> Result<String>
    where
        F: Fn(&str),
    {
        let (orig_name, mut graph, config) = match format {
            ImportFormat::RelANNIS => relannis::load(path, disk_based, |status| {
                progress_callback(status);
                // loading the file from relANNIS consumes memory, update the corpus cache regularly to allow it to adapt
                self.check_cache_size_and_remove(vec![], false);
            })?,
            ImportFormat::GraphML => {
                let orig_corpus_name = if let Some(file_name) = path.file_stem() {
                    file_name.to_string_lossy().to_string()
                } else {
                    "UnknownCorpus".to_string()
                };
                let input_file = File::open(path)?;
                let (g, config_str) = graphannis_core::graph::serialization::graphml::import(
                    input_file,
                    disk_based,
                    |status| {
                        progress_callback(status);
                        // loading the file from relANNIS consumes memory, update the corpus cache regularly to allow it to adapt
                        self.check_cache_size_and_remove(vec![], false);
                    },
                )?;
                let config = if let Some(config_str) = config_str {
                    toml::from_str(&config_str)?
                } else {
                    CorpusConfiguration::default()
                };
                (orig_corpus_name.into(), g, config)
            }
        };

        let r = graph.ensure_loaded_all();
        if let Err(e) = r {
            error!(
                "Some error occurred when attempting to load components from disk: {:?}",
                e
            );
        }

        let corpus_name = corpus_name.unwrap_or_else(|| orig_name.into());
        let escaped_corpus_name: Cow<str> =
            utf8_percent_encode(&corpus_name, PATH_SEGMENT_ENCODE_SET).into();

        let mut db_path = PathBuf::from(&self.db_dir);
        db_path.push(escaped_corpus_name.to_string());

        let mut cache_lock = self.corpus_cache.write().unwrap();
        let cache = &mut *cache_lock;

        // make sure the cache is not too large before adding the new corpus
        check_cache_size_and_remove_with_cache(cache, &self.cache_strategy, vec![], false);

        // remove any possible old corpus
        if cache.contains_key(&corpus_name) {
            if overwrite_existing {
                let old_entry = cache.remove(&corpus_name);
                if old_entry.is_some() {
                    if let Err(e) = std::fs::remove_dir_all(db_path.clone()) {
                        error!("Error when removing existing files {}", e);
                    }
                }
            } else {
                return Err(GraphAnnisError::CorpusExists(corpus_name.to_string()));
            }
        }

        if let Err(e) = std::fs::create_dir_all(&db_path) {
            error!(
                "Can't create directory {}: {:?}",
                db_path.to_string_lossy(),
                e
            );
        }

        info!("copying linked files for corpus {}", corpus_name);
        let current_dir = PathBuf::from(".");
        let files_dir = db_path.join("files");
        std::fs::create_dir_all(&files_dir)?;
        self.copy_linked_files_and_update_references(
            path.parent().unwrap_or(&current_dir),
            &files_dir,
            &mut graph,
        )?;

        // save to its location
        info!("saving corpus {} to disk", corpus_name);
        let save_result = graph.save_to(&db_path);
        if let Err(e) = save_result {
            error!(
                "Can't save corpus to {}: {:?}",
                db_path.to_string_lossy(),
                e
            );
        }

        // Use the imported/generated/default corpus configuration and store it in our graph directory
        let corpus_config_path = db_path.join("corpus-config.toml");
        info!(
            "saving corpus configuration file for corpus {} to {}",
            corpus_name,
            &corpus_config_path.to_string_lossy()
        );
        std::fs::write(corpus_config_path, toml::to_string(&config)?)?;

        // make it known to the cache
        cache.insert(
            corpus_name.clone(),
            Arc::new(RwLock::new(CacheEntry::Loaded(graph))),
        );
        check_cache_size_and_remove_with_cache(
            cache,
            &self.cache_strategy,
            vec![&corpus_name],
            true,
        );

        Ok(corpus_name)
    }

    fn copy_linked_files_and_update_references(
        &self,
        old_base_path: &Path,
        new_base_path: &Path,
        graph: &mut AnnotationGraph,
    ) -> Result<()> {
        let linked_file_key = AnnoKey {
            ns: ANNIS_NS.into(),
            name: "file".into(),
        };
        // Find all nodes of the type "file"
        let node_annos: &mut dyn AnnotationStorage<NodeID> = graph.get_node_annos_mut();
        let file_nodes: Vec<NodeID> = node_annos
            .exact_anno_search(Some(ANNIS_NS), NODE_TYPE, ValueSearch::Some("file"))
            .map(|m| m.node)
            .collect();
        for node in file_nodes {
            // Get the linked file for this node
            if let Some(original_path) = node_annos.get_value_for_item(&node, &linked_file_key) {
                let original_path = old_base_path
                    .canonicalize()?
                    .join(&PathBuf::from(original_path.as_ref()));
                if original_path.is_file() {
                    if let Some(node_name) = node_annos.get_value_for_item(&node, &NODE_NAME_KEY) {
                        // Create a new file name based on the node name and copy the file
                        let new_path = new_base_path.join(node_name.as_ref());
                        if let Some(parent) = new_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::copy(&original_path, &new_path)?;
                        // Update the annotation to link to the new file with a relative path.
                        // Use the corpus directory as base path for this relative path.
                        let relative_path = new_path.strip_prefix(&new_base_path)?;
                        node_annos.insert(
                            node,
                            Annotation {
                                key: linked_file_key.clone(),
                                val: relative_path.to_string_lossy().into(),
                            },
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Find all nodes of the type "file" and return an iterator
    /// over a tuple of the node name and the absolute path of the linked file.
    fn get_linked_files<'a>(
        &'a self,
        corpus_name: &'a str,
        graph: &'a AnnotationGraph,
    ) -> Result<impl Iterator<Item = (String, PathBuf)> + 'a> {
        let linked_file_key = AnnoKey {
            ns: ANNIS_NS.into(),
            name: "file".into(),
        };

        let base_path = self.db_dir.join(corpus_name).join("files").canonicalize()?;

        // Find all nodes of the type "file"
        let node_annos: &dyn AnnotationStorage<NodeID> = graph.get_node_annos();
        let it = node_annos
            .exact_anno_search(Some(ANNIS_NS), NODE_TYPE, ValueSearch::Some("file"))
            // Get the linked file for this node
            .filter_map(move |m| {
                if let Some(node_name) = node_annos.get_value_for_item(&m.node, &NODE_NAME_KEY) {
                    if let Some(file_path_value) =
                        node_annos.get_value_for_item(&m.node, &linked_file_key)
                    {
                        return Some((
                            node_name.to_string(),
                            base_path.join(file_path_value.to_string()),
                        ));
                    }
                }
                None
            });
        Ok(it)
    }

    fn copy_linked_files_to_disk(
        &self,
        corpus_name: &str,
        new_base_path: &Path,
        graph: &AnnotationGraph,
    ) -> Result<()> {
        for (node_name, original_path) in self.get_linked_files(corpus_name, graph)? {
            let node_name: String = node_name;
            if original_path.is_file() {
                // Create a new file name based on the node name and copy the file
                let new_path = new_base_path.join(&node_name);
                if let Some(parent) = new_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&original_path, &new_path)?;
            }
        }
        Ok(())
    }

    fn export_corpus_graphml(&self, corpus_name: &str, path: &Path) -> Result<()> {
        let output_file = File::create(path)?;
        let entry = self.get_loaded_entry(corpus_name, false)?;

        // Ensure all components are loaded
        {
            let mut lock = entry.write().unwrap();
            let graph: &mut AnnotationGraph = get_write_or_error(&mut lock)?;
            graph.ensure_loaded_all()?;
        }
        // Perform the export on a read-only reference
        let lock = entry.read().unwrap();
        let graph: &AnnotationGraph = get_read_or_error(&lock)?;

        let config_as_str = if let Some(config) = self.get_corpus_config(corpus_name)? {
            Some(toml::to_string_pretty(&config)?)
        } else {
            None
        };

        let config_as_str = config_as_str.as_deref();
        graphannis_core::graph::serialization::graphml::export(
            graph,
            config_as_str,
            output_file,
            |status| {
                info!("{}", status);
            },
        )?;

        if let Some(parent_dir) = path.parent() {
            self.copy_linked_files_to_disk(corpus_name, &parent_dir, &graph)?;
        }

        Ok(())
    }

    pub fn export_corpus_zip<W, F>(
        &self,
        corpus_name: &str,
        use_corpus_subdirectory: bool,
        mut zip: &mut zip::ZipWriter<W>,
        progress_callback: F,
    ) -> Result<()>
    where
        W: Write + Seek,
        F: Fn(&str),
    {
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut base_path = PathBuf::default();
        if use_corpus_subdirectory {
            base_path.push(corpus_name);
        }
        let path_in_zip = base_path.join(format!("{}.graphml", corpus_name));
        zip.start_file_from_path(&path_in_zip, options)?;

        let entry = self.get_loaded_entry(corpus_name, false)?;

        // Ensure all components are loaded
        {
            let mut lock = entry.write().unwrap();
            let graph: &mut AnnotationGraph = get_write_or_error(&mut lock)?;
            graph.ensure_loaded_all()?;
        }
        // Perform the export on a read-only reference
        let lock = entry.read().unwrap();
        let graph: &AnnotationGraph = get_read_or_error(&lock)?;

        let config_as_str = if let Some(config) = self.get_corpus_config(corpus_name)? {
            Some(toml::to_string_pretty(&config)?)
        } else {
            None
        };

        let config_as_str: Option<&str> = config_as_str.as_deref();
        graphannis_core::graph::serialization::graphml::export(
            graph,
            config_as_str,
            &mut zip,
            progress_callback,
        )?;

        // Insert all linked files into the ZIP file
        for (node_name, original_path) in self.get_linked_files(corpus_name.as_ref(), graph)? {
            let node_name: String = node_name;

            zip.start_file_from_path(&base_path.join(&node_name), options)?;
            let file_to_copy = File::open(original_path)?;
            let mut reader = BufReader::new(file_to_copy);
            std::io::copy(&mut reader, zip)?;
        }

        Ok(())
    }

    pub fn export_to_fs<S: AsRef<str>>(
        &self,
        corpora: &[S],
        path: &Path,
        format: ExportFormat,
    ) -> Result<()> {
        match format {
            ExportFormat::GraphML => {
                if corpora.len() == 1 {
                    self.export_corpus_graphml(corpora[0].as_ref(), path)?;
                } else {
                    return Err(CorpusStorageError::MultipleCorporaForSingleCorpusFormat(
                        corpora.len(),
                    )
                    .into());
                }
            }
            ExportFormat::GraphMLDirectory => {
                let use_corpus_subdirectory = corpora.len() > 1;
                for corpus_name in corpora {
                    let mut path = PathBuf::from(path);
                    if use_corpus_subdirectory {
                        // Use a sub-directory with the corpus name to avoid conflicts with the
                        // linked files
                        path.push(corpus_name.as_ref());
                    } else {
                    };
                    std::fs::create_dir_all(&path)?;
                    path.push(format!("{}.graphml", corpus_name.as_ref()));
                    self.export_corpus_graphml(corpus_name.as_ref(), &path)?;
                }
            }
            ExportFormat::GraphMLZip => {
                let output_file = File::create(path)?;
                let mut zip = zip::ZipWriter::new(output_file);

                let use_corpus_subdirectory = corpora.len() > 1;
                for corpus_name in corpora {
                    // Add the GraphML file to the ZIP file
                    let corpus_name: &str = corpus_name.as_ref();
                    self.export_corpus_zip(
                        corpus_name,
                        use_corpus_subdirectory,
                        &mut zip,
                        |status| {
                            info!("{}", status);
                        },
                    )?;
                }

                zip.finish()?;
            }
        }

        Ok(())
    }

    /// Delete a corpus from this corpus storage.
    /// Returns `true` if the corpus was successfully deleted and `false` if no such corpus existed.
    pub fn delete(&self, corpus_name: &str) -> Result<bool> {
        let mut db_path = PathBuf::from(&self.db_dir);
        db_path.push(corpus_name);

        let mut cache_lock = self.corpus_cache.write().unwrap();

        let cache = &mut *cache_lock;

        // remove any possible old corpus
        if let Some(db_entry) = cache.remove(corpus_name) {
            // aquire exclusive lock for this cache entry because
            // other queries or background writer might still have access it and need to finish first
            let mut _lock = db_entry.write().unwrap();

            if db_path.is_dir() && db_path.exists() {
                std::fs::remove_dir_all(db_path).map_err(|e| {
                    CorpusStorageError::RemoveFileForCorpus {
                        corpus: corpus_name.to_string(),
                        source: e,
                    }
                })?
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Apply a sequence of updates (`update` parameter) to this graph for a corpus given by the `corpus_name` parameter.
    ///
    /// It is ensured that the update process is atomic and that the changes are persisted to disk if the result is `Ok`.
    pub fn apply_update(&self, corpus_name: &str, update: &mut GraphUpdate) -> Result<()> {
        let db_entry = self.get_loaded_entry(corpus_name, true)?;
        {
            let mut lock = db_entry.write().unwrap();
            let db: &mut AnnotationGraph = get_write_or_error(&mut lock)?;

            db.apply_update(update, |_| {})?;
        }
        // start background thread to persists the results

        let active_background_workers = self.active_background_workers.clone();
        {
            let &(ref lock, ref _cvar) = &*active_background_workers;
            let mut nr_active_background_workers = lock.lock().unwrap();
            *nr_active_background_workers += 1;
        }
        thread::spawn(move || {
            trace!("Starting background thread to sync WAL updates");
            let lock = db_entry.read().unwrap();
            if let Ok(db) = get_read_or_error(&lock) {
                let db: &AnnotationGraph = db;
                if let Err(e) = db.background_sync_wal_updates() {
                    error!("Can't sync changes in background thread: {:?}", e);
                } else {
                    trace!("Finished background thread to sync WAL updates");
                }
            }
            let &(ref lock, ref cvar) = &*active_background_workers;
            let mut nr_active_background_workers = lock.lock().unwrap();
            *nr_active_background_workers -= 1;
            cvar.notify_all();
        });

        Ok(())
    }

    fn prepare_query<'a, F>(
        &self,
        corpus_name: &str,
        query: &'a str,
        query_language: QueryLanguage,
        additional_components_callback: F,
    ) -> Result<PreparationResult<'a>>
    where
        F: FnOnce(&AnnotationGraph) -> Vec<Component<AnnotationComponentType>>,
    {
        let db_entry = self.get_loaded_entry(corpus_name, false)?;

        // make sure the database is loaded with all necessary components
        let (q, missing_components) = {
            let lock = db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;

            let q = match query_language {
                QueryLanguage::AQL => aql::parse(query, false)?,
                QueryLanguage::AQLQuirksV3 => aql::parse(query, true)?,
            };

            let necessary_components = q.necessary_components(db);

            let mut missing: HashSet<_> = necessary_components.iter().cloned().collect();

            let additional_components = additional_components_callback(db);

            // make sure the additional components are loaded
            missing.extend(additional_components.into_iter());

            // remove all that are already loaded
            for c in &necessary_components {
                if db.get_graphstorage(c).is_some() {
                    missing.remove(c);
                }
            }
            let missing: Vec<_> = missing.into_iter().collect();
            (q, missing)
        };

        if !missing_components.is_empty() {
            // load the needed components
            {
                let mut lock = db_entry.write().unwrap();
                let db = get_write_or_error(&mut lock)?;
                for c in missing_components {
                    db.ensure_loaded(&c)?;
                }
            }
            self.check_cache_size_and_remove(vec![corpus_name], true);
        };

        Ok(PreparationResult { query: q, db_entry })
    }

    /// Preloads all annotation and graph storages from the disk into a main memory cache.
    pub fn preload(&self, corpus_name: &str) -> Result<()> {
        {
            let db_entry = self.get_loaded_entry(corpus_name, false)?;
            let mut lock = db_entry.write().unwrap();
            let db = get_write_or_error(&mut lock)?;
            db.ensure_loaded_all()?;
        }
        self.check_cache_size_and_remove(vec![corpus_name], true);
        Ok(())
    }

    /// Unloads a corpus from the cache.
    pub fn unload(&self, corpus_name: &str) {
        let mut cache_lock = self.corpus_cache.write().unwrap();
        let cache = &mut *cache_lock;
        cache.remove(corpus_name);
    }

    /// Optimize the node annotation and graph storage implementations of the given corpus.
    /// - `corpus_name` - The corpus name to optimize.
    /// - `disk_based` - If `true`, prefer disk-based annotation and graph storages instead of memory-only ones.
    #[doc(hidden)]
    pub fn reoptimize_implementation(&self, corpus_name: &str, disk_based: bool) -> Result<()> {
        let graph_entry = self.get_loaded_entry(corpus_name, false)?;
        let mut lock = graph_entry.write().unwrap();
        let graph: &mut AnnotationGraph = get_write_or_error(&mut lock)?;

        graph.optimize_impl(disk_based)?;
        Ok(())
    }

    /// Parses a `query` and checks if it is valid.
    ///
    /// - `corpus_names` - The name of the corpora the query would be executed on (needed to catch certain corpus-specific semantic errors).
    /// - `query` - The query as string.
    /// - `query_language` The query language of the query (e.g. AQL).
    ///
    /// Returns `true` if valid and an error with the parser message if invalid.
    pub fn validate_query<S: AsRef<str>>(
        &self,
        corpus_names: &[S],
        query: &str,
        query_language: QueryLanguage,
    ) -> Result<bool> {
        for cn in corpus_names {
            let prep: PreparationResult =
                self.prepare_query(cn.as_ref(), query, query_language, |_| vec![])?;
            // also get the semantic errors by creating an execution plan on the actual Graph
            let lock = prep.db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;
            ExecutionPlan::from_disjunction(&prep.query, &db, &self.query_config)?;
        }
        Ok(true)
    }

    /// Returns a string representation of the execution plan for a `query`.
    ///
    /// - `corpus_names` - The name of the corpora to execute the query on.
    /// - `query` - The query as string.
    /// - `query_language` The query language of the query (e.g. AQL).
    pub fn plan<S: AsRef<str>>(
        &self,
        corpus_names: &[S],
        query: &str,
        query_language: QueryLanguage,
    ) -> Result<String> {
        let mut all_plans = Vec::with_capacity(corpus_names.len());
        for cn in corpus_names {
            let prep = self.prepare_query(cn.as_ref(), query, query_language, |_| vec![])?;

            // acquire read-only lock and plan
            let lock = prep.db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;
            let plan = ExecutionPlan::from_disjunction(&prep.query, &db, &self.query_config)?;

            all_plans.push(format!("{}:\n{}", cn.as_ref(), plan));
        }
        Ok(all_plans.join("\n"))
    }

    /// Count the number of results for a `query`.
    /// - `query` - The search query definition.
    /// Returns the count as number.
    pub fn count<S: AsRef<str>>(&self, query: SearchQuery<S>) -> Result<u64> {
        let timeout = TimeoutCheck::new(query.timeout);
        let mut total_count: u64 = 0;

        for cn in query.corpus_names {
            let prep =
                self.prepare_query(cn.as_ref(), query.query, query.query_language, |_| vec![])?;

            // acquire read-only lock and execute query
            let lock = prep.db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;
            let plan = ExecutionPlan::from_disjunction(&prep.query, &db, &self.query_config)?;

            for _ in plan {
                total_count += 1;
                if total_count % 1_000 == 0 {
                    timeout.check()?;
                }
            }

            timeout.check()?;
        }

        Ok(total_count)
    }

    /// Count the number of results for a `query` and return both the total number of matches and also the number of documents in the result set.
    ///
    /// - `query` - The search query definition.
    pub fn count_extra<S: AsRef<str>>(&self, query: SearchQuery<S>) -> Result<CountExtra> {
        let timeout = TimeoutCheck::new(query.timeout);

        let mut match_count: u64 = 0;
        let mut document_count: u64 = 0;

        for cn in query.corpus_names {
            let prep =
                self.prepare_query(cn.as_ref(), query.query, query.query_language, |_| vec![])?;

            // acquire read-only lock and execute query
            let lock = prep.db_entry.read().unwrap();
            let db: &AnnotationGraph = get_read_or_error(&lock)?;
            let plan = ExecutionPlan::from_disjunction(&prep.query, &db, &self.query_config)?;

            let mut known_documents: HashSet<SmartString> = HashSet::new();

            for m in plan {
                if !m.is_empty() {
                    let m: &Match = &m[0];
                    if let Some(node_name) = db
                        .get_node_annos()
                        .get_value_for_item(&m.node, &NODE_NAME_KEY)
                    {
                        let node_name: &str = &node_name;
                        // extract the document path from the node name
                        let doc_path =
                            &node_name[0..node_name.rfind('#').unwrap_or_else(|| node_name.len())];
                        known_documents.insert(doc_path.into());
                    }
                }
                match_count += 1;

                if match_count % 1_000 == 0 {
                    timeout.check()?;
                }
            }
            document_count += known_documents.len() as u64;

            timeout.check()?;
        }

        Ok(CountExtra {
            match_count,
            document_count,
        })
    }

    fn create_find_iterator_for_query<'b>(
        &'b self,
        db: &'b AnnotationGraph,
        query: &'b Disjunction,
        offset: usize,
        limit: Option<usize>,
        order: ResultOrder,
        quirks_mode: bool,
    ) -> Result<(FindIterator<'b>, Option<usize>)> {
        let mut query_config = self.query_config.clone();
        if order == ResultOrder::NotSorted {
            // Do execute query in parallel if the order should not be sorted to have a more stable result ordering.
            // Even if we do not promise to have a stable ordering, it should be the same
            // for the same session on the same corpus.
            query_config.use_parallel_joins = false;
        }

        let plan = ExecutionPlan::from_disjunction(query, &db, &query_config)?;

        // Try to find the relANNIS version by getting the attribute value which should be attached to the
        // toplevel corpus node.
        let mut relannis_version_33 = false;
        if quirks_mode {
            let mut relannis_version_it = db.get_node_annos().exact_anno_search(
                Some(ANNIS_NS),
                "relannis-version",
                ValueSearch::Any,
            );
            if let Some(m) = relannis_version_it.next() {
                if let Some(v) = db.get_node_annos().get_value_for_item(&m.node, &m.anno_key) {
                    if v == "3.3" {
                        relannis_version_33 = true;
                    }
                }
            }
        }
        let mut expected_size: Option<usize> = None;
        let base_it: FindIterator = if order == ResultOrder::NotSorted
            || (order == ResultOrder::Normal && plan.is_sorted_by_text() && !quirks_mode)
        {
            // If the output is already sorted correctly, directly return the iterator.
            // Quirks mode may change the order of the results, thus don't use the shortcut
            // if quirks mode is active.
            Box::from(plan)
        } else {
            let estimated_result_size = plan.estimated_output_size();
            // Estimations can be wrong on the upper limit, so limit the maximal reserved vector size
            let expected_len = std::cmp::min(estimated_result_size, MAX_VECTOR_RESERVATION);
            let mut tmp_results: Vec<MatchGroup> =
                new_vector_with_memory_aligned_capacity(expected_len);

            for mgroup in plan {
                // add all matches to temporary vector
                tmp_results.push(mgroup);
            }

            // either sort or randomly shuffle results
            if order == ResultOrder::Randomized {
                let mut rng = rand::thread_rng();
                tmp_results.shuffle(&mut rng);
            } else {
                let token_helper = TokenHelper::new(db);
                let component_order = Component::new(
                    AnnotationComponentType::Ordering,
                    ANNIS_NS.into(),
                    "".into(),
                );

                let collation = if quirks_mode && !relannis_version_33 {
                    CollationType::Locale
                } else {
                    CollationType::Default
                };

                let gs_order = db.get_graphstorage_as_ref(&component_order);
                let order_func = |m1: &MatchGroup, m2: &MatchGroup| -> std::cmp::Ordering {
                    if order == ResultOrder::Inverted {
                        db::sort_matches::compare_matchgroup_by_text_pos(
                            m1,
                            m2,
                            db.get_node_annos(),
                            token_helper.as_ref(),
                            gs_order,
                            collation,
                            quirks_mode,
                        )
                        .reverse()
                    } else {
                        db::sort_matches::compare_matchgroup_by_text_pos(
                            m1,
                            m2,
                            db.get_node_annos(),
                            token_helper.as_ref(),
                            gs_order,
                            collation,
                            quirks_mode,
                        )
                    }
                };

                let sort_size = if let Some(limit) = limit {
                    // we won't need to sort all items
                    offset + limit
                } else {
                    // sort all items if unlimited iterator is requested
                    tmp_results.len()
                };

                if self.query_config.use_parallel_joins {
                    quicksort::sort_first_n_items_parallel(&mut tmp_results, sort_size, order_func);
                } else {
                    quicksort::sort_first_n_items(&mut tmp_results, sort_size, order_func);
                }
            }
            expected_size = Some(tmp_results.len());
            Box::from(tmp_results.into_iter())
        };

        Ok((base_it, expected_size))
    }

    fn find_in_single_corpus<S: AsRef<str>>(
        &self,
        query: &SearchQuery<S>,
        corpus_name: &str,
        offset: usize,
        limit: Option<usize>,
        order: ResultOrder,
        timeout: TimeoutCheck,
    ) -> Result<(Vec<String>, usize)> {
        let prep = self.prepare_query(corpus_name, query.query, query.query_language, |db| {
            let mut additional_components = vec![Component::new(
                AnnotationComponentType::Ordering,
                ANNIS_NS.into(),
                "".into(),
            )];
            if order == ResultOrder::Normal || order == ResultOrder::Inverted {
                for c in token_helper::necessary_components(db) {
                    additional_components.push(c);
                }
            }
            additional_components
        })?;

        // acquire read-only lock and execute query
        let lock = prep.db_entry.read().unwrap();
        let db = get_read_or_error(&lock)?;

        let quirks_mode = match query.query_language {
            QueryLanguage::AQL => false,
            QueryLanguage::AQLQuirksV3 => true,
        };

        let (mut base_it, expected_size) = self.create_find_iterator_for_query(
            db,
            &prep.query,
            offset,
            limit,
            order,
            quirks_mode,
        )?;

        let mut results: Vec<String> = if let Some(expected_size) = expected_size {
            new_vector_with_memory_aligned_capacity(expected_size)
        } else if let Some(limit) = limit {
            new_vector_with_memory_aligned_capacity(limit)
        } else {
            Vec::new()
        };

        // skip the first entries
        let mut skipped = 0;
        while skipped < offset && base_it.next().is_some() {
            skipped += 1;

            if skipped % 1_000 == 0 {
                timeout.check()?;
            }
        }
        let base_it: Box<dyn Iterator<Item = MatchGroup>> = if let Some(limit) = limit {
            Box::new(base_it.take(limit))
        } else {
            Box::new(base_it)
        };

        for (match_nr, m) in base_it.enumerate() {
            let mut match_desc = String::new();

            for (i, singlematch) in m.iter().enumerate() {
                // check if query node actually should be included in quirks mode
                let include_in_output = if quirks_mode {
                    if let Some(var) = prep.query.get_variable_by_pos(i) {
                        prep.query.is_included_in_output(&var)
                    } else {
                        true
                    }
                } else {
                    true
                };

                if include_in_output {
                    if i > 0 {
                        match_desc.push(' ');
                    }

                    let singlematch_anno_key = &singlematch.anno_key;
                    if singlematch_anno_key.ns != ANNIS_NS || singlematch_anno_key.name != NODE_TYPE
                    {
                        if !singlematch_anno_key.ns.is_empty() {
                            let encoded_anno_ns: Cow<str> =
                                utf8_percent_encode(&singlematch_anno_key.ns, SALT_URI_ENCODE_SET)
                                    .into();
                            match_desc.push_str(&encoded_anno_ns);
                            match_desc.push_str("::");
                        }
                        let encoded_anno_name: Cow<str> =
                            utf8_percent_encode(&singlematch_anno_key.name, SALT_URI_ENCODE_SET)
                                .into();
                        match_desc.push_str(&encoded_anno_name);
                        match_desc.push_str("::");
                    }

                    if let Some(name) = db
                        .get_node_annos()
                        .get_value_for_item(&singlematch.node, &NODE_NAME_KEY)
                    {
                        if quirks_mode {
                            // Unescape and re-escape with quirks-mode compatible character encoding set
                            let decoded_name =
                                percent_encoding::percent_decode_str(&name).decode_utf8_lossy();
                            let re_encoded_name: Cow<str> =
                                utf8_percent_encode(&decoded_name, QUIRKS_SALT_URI_ENCODE_SET)
                                    .into();
                            match_desc.push_str(&re_encoded_name);
                        } else {
                            match_desc.push_str(&name);
                        }
                    }
                }
            }
            results.push(match_desc);
            if match_nr % 1_000 == 0 {
                timeout.check()?;
            }
        }

        Ok((results, skipped))
    }

    /// Find all results for a `query` and return the match ID for each result.
    ///
    /// The query is paginated and an offset and limit can be specified.
    ///
    /// - `query` - The search query definition.
    /// - `offset` - Skip the `n` first results, where `n` is the offset.
    /// - `limit` - Return at most `n` matches, where `n` is the limit.  Use `None` to allow unlimited result sizes.
    /// - `order` - Specify the order of the matches.
    ///
    /// Returns a vector of match IDs, where each match ID consists of the matched node annotation identifiers separated by spaces.
    /// You can use the [subgraph(...)](#method.subgraph) method to get the subgraph for a single match described by the node annnotation identifiers.
    pub fn find<S: AsRef<str>>(
        &self,
        query: SearchQuery<S>,
        offset: usize,
        limit: Option<usize>,
        order: ResultOrder,
    ) -> Result<Vec<String>> {
        let timeout = TimeoutCheck::new(query.timeout);

        // Sort corpus names
        let mut corpus_names: Vec<SmartString> = query
            .corpus_names
            .iter()
            .map(|c| c.as_ref().into())
            .collect();

        match corpus_names.len() {
            0 => Ok(Vec::new()),
            1 => self
                .find_in_single_corpus(
                    &query,
                    corpus_names[0].as_str(),
                    offset,
                    limit,
                    order,
                    timeout,
                )
                .map(|r| r.0),
            _ => {
                if order == ResultOrder::Randomized {
                    // This is still oddly ordered, because results from one corpus will always be grouped together.
                    // But it still better than just output the same corpus first.
                    let mut rng = rand::thread_rng();
                    corpus_names.shuffle(&mut rng);
                } else if order == ResultOrder::Inverted {
                    corpus_names.sort();
                    corpus_names.reverse();
                } else {
                    corpus_names.sort();
                }

                // initialize the limit/offset values for the first corpus
                let mut offset = offset;
                let mut limit = limit;

                let mut result = Vec::new();
                for cn in corpus_names {
                    let (single_result, skipped) = self.find_in_single_corpus(
                        &query,
                        cn.as_ref(),
                        offset,
                        limit,
                        order,
                        timeout,
                    )?;

                    // Adjust limit and offset according to the found matches for the next corpus.
                    let single_result_length = single_result.len();
                    result.extend(single_result.into_iter());

                    if let Some(current_limit) = limit {
                        if current_limit <= single_result_length {
                            // Searching in this corpus already yielded enough results
                            break;
                        } else {
                            // Adjust the limit for the next corpora to the already found results so-far
                            limit = Some(current_limit - single_result_length);
                        }
                    }
                    if skipped < offset {
                        offset -= skipped;
                    } else {
                        offset = 0;
                    }

                    timeout.check()?;
                }
                Ok(result)
            }
        }
    }

    /// Return the copy of a subgraph which includes the given list of node annotation identifiers,
    /// the nodes that cover the same token as the given nodes and
    /// all nodes that cover the token which are part of the defined context.
    ///
    /// - `corpus_name` - The name of the corpus for which the subgraph should be generated from.
    /// - `node_ids` - A set of node annotation identifiers describing the subgraph.
    /// - `ctx_left` and `ctx_right` - Left and right context in token distance to be included in the subgraph.
    /// - `segmentation` - The name of the segmentation which should be used to as base for the context. Use `None` to define the context in the default token layer.
    pub fn subgraph(
        &self,
        corpus_name: &str,
        node_ids: Vec<String>,
        ctx_left: usize,
        ctx_right: usize,
        segmentation: Option<String>,
    ) -> Result<AnnotationGraph> {
        let db_entry = self.get_fully_loaded_entry(corpus_name)?;

        let mut query = Disjunction {
            alternatives: vec![],
        };

        // find all nodes covering the same token
        for source_node_id in node_ids {
            // remove the obsolete "salt:/" prefix
            let source_node_id: &str = source_node_id
                .strip_prefix("salt:/")
                .unwrap_or(&source_node_id);

            let m = NodeSearchSpec::ExactValue {
                ns: Some(ANNIS_NS.to_string()),
                name: NODE_NAME.to_string(),
                val: Some(source_node_id.to_string()),
                is_meta: false,
            };

            // nodes overlapping the match: m _o_ node
            {
                let mut q = Conjunction::new();
                let node_idx = q.add_node(NodeSearchSpec::AnyNode, None);
                let m_idx = q.add_node(m.clone(), None);
                q.add_operator(
                    Box::new(operators::OverlapSpec { reflexive: true }),
                    &m_idx,
                    &node_idx,
                    false,
                )?;
                query.alternatives.push(q);
            }

            // token left/right and their overlapped nodes
            if let Some(ref segmentation) = segmentation {
                add_subgraph_precedence_with_segmentation(
                    &mut query,
                    ctx_left,
                    segmentation,
                    &m,
                    true,
                )?;
                add_subgraph_precedence_with_segmentation(
                    &mut query,
                    ctx_right,
                    segmentation,
                    &m,
                    false,
                )?;
            } else {
                add_subgraph_precedence(&mut query, ctx_left, &m, true)?;
                add_subgraph_precedence(&mut query, ctx_right, &m, false)?;
            }

            // add the textual data sources (which are not part of the corpus graph)
            {
                let mut q = Conjunction::new();
                let datasource_idx = q.add_node(
                    NodeSearchSpec::ExactValue {
                        ns: Some(ANNIS_NS.to_string()),
                        name: NODE_TYPE.to_string(),
                        val: Some("datasource".to_string()),
                        is_meta: false,
                    },
                    None,
                );
                let m_idx = q.add_node(m.clone(), None);
                q.add_operator(
                    Box::new(operators::PartOfSubCorpusSpec {
                        dist: RangeSpec::Bound {
                            min_dist: 1,
                            max_dist: 1,
                        },
                    }),
                    &m_idx,
                    &datasource_idx,
                    false,
                )?;
                query.alternatives.push(q);
            }
        }
        extract_subgraph_by_query(&db_entry, &query, &[0], &self.query_config, None)
    }

    /// Return the copy of a subgraph which includes all nodes matched by the given `query`.
    ///
    /// - `corpus_name` - The name of the corpus for which the subgraph should be generated from.
    /// - `query` - The query which defines included nodes.
    /// - `query_language` - The query language of the query (e.g. AQL).
    /// - `component_type_filter` - If set, only include edges of that belong to a component of the given type.
    pub fn subgraph_for_query(
        &self,
        corpus_name: &str,
        query: &str,
        query_language: QueryLanguage,
        component_type_filter: Option<AnnotationComponentType>,
    ) -> Result<AnnotationGraph> {
        let prep = self.prepare_query(corpus_name, query, query_language, |g| {
            g.get_all_components(component_type_filter.clone(), None)
        })?;

        let mut max_alt_size = 0;
        for alt in &prep.query.alternatives {
            max_alt_size = std::cmp::max(max_alt_size, alt.num_of_nodes());
        }

        let match_idx: Vec<usize> = (0..max_alt_size).collect();

        extract_subgraph_by_query(
            &prep.db_entry,
            &prep.query,
            &match_idx,
            &self.query_config,
            component_type_filter,
        )
    }

    /// Return the copy of a subgraph which includes all nodes that belong to any of the given list of sub-corpus/document identifiers.
    ///
    /// - `corpus_name` - The name of the corpus for which the subgraph should be generated from.
    /// - `corpus_ids` - A set of sub-corpus/document identifiers describing the subgraph.
    pub fn subcorpus_graph(
        &self,
        corpus_name: &str,
        corpus_ids: Vec<String>,
    ) -> Result<AnnotationGraph> {
        let db_entry = self.get_fully_loaded_entry(corpus_name)?;

        let mut query = Disjunction {
            alternatives: vec![],
        };
        // find all nodes that a connected with the corpus IDs
        for source_corpus_id in corpus_ids {
            // remove the obsolete "salt:/" prefix
            let source_corpus_id: &str = source_corpus_id
                .strip_prefix("salt:/")
                .unwrap_or(&source_corpus_id);
            // All annotation nodes
            {
                let mut q = Conjunction::new();
                let corpus_idx = q.add_node(
                    NodeSearchSpec::ExactValue {
                        ns: Some(ANNIS_NS.to_string()),
                        name: NODE_NAME.to_string(),
                        val: Some(source_corpus_id.to_string()),
                        is_meta: false,
                    },
                    None,
                );
                let any_node_idx = q.add_node(NodeSearchSpec::AnyNode, None);
                q.add_operator(
                    Box::new(operators::PartOfSubCorpusSpec {
                        dist: RangeSpec::Unbound,
                    }),
                    &any_node_idx,
                    &corpus_idx,
                    true,
                )?;
                query.alternatives.push(q);
            }
            // All data source nodes
            {
                let mut q = Conjunction::new();
                let corpus_idx = q.add_node(
                    NodeSearchSpec::ExactValue {
                        ns: Some(ANNIS_NS.to_string()),
                        name: NODE_NAME.to_string(),
                        val: Some(source_corpus_id.to_string()),
                        is_meta: false,
                    },
                    None,
                );
                let any_node_idx = q.add_node(
                    NodeSearchSpec::ExactValue {
                        ns: Some(ANNIS_NS.to_string()),
                        name: NODE_TYPE.to_string(),
                        val: Some("datasource".to_string()),
                        is_meta: false,
                    },
                    None,
                );
                q.add_operator(
                    Box::new(operators::PartOfSubCorpusSpec {
                        dist: RangeSpec::Unbound,
                    }),
                    &any_node_idx,
                    &corpus_idx,
                    true,
                )?;
                query.alternatives.push(q);
            }
        }

        extract_subgraph_by_query(&db_entry, &query, &[1], &self.query_config, None)
    }

    /// Return the copy of the graph of the corpus structure given by `corpus_name`.
    pub fn corpus_graph(&self, corpus_name: &str) -> Result<AnnotationGraph> {
        let db_entry = self.get_loaded_entry(corpus_name, false)?;

        let subcorpus_components = {
            // make sure all subcorpus partitions are loaded
            let lock = db_entry.read().unwrap();
            let db = get_read_or_error(&lock)?;
            db.get_all_components(Some(AnnotationComponentType::PartOf), None)
        };
        let db_entry = self.get_loaded_entry_with_components(corpus_name, subcorpus_components)?;

        let mut query = Conjunction::new();

        query.add_node(
            NodeSearchSpec::new_exact(Some(ANNIS_NS), NODE_TYPE, Some("corpus"), false),
            None,
        );

        extract_subgraph_by_query(
            &db_entry,
            &query.into_disjunction(),
            &[0],
            &self.query_config,
            Some(AnnotationComponentType::PartOf),
        )
    }

    /// Execute a frequency query.
    ///
    /// - `query` - The search query definition.
    /// - `definition` - A list of frequency query definitions.
    ///
    /// Returns a frequency table of strings.
    pub fn frequency<S: AsRef<str>>(
        &self,
        query: SearchQuery<S>,
        definition: Vec<FrequencyDefEntry>,
    ) -> Result<FrequencyTable<String>> {
        let timeout = TimeoutCheck::new(query.timeout);

        let mut tuple_frequency: FxHashMap<Vec<String>, usize> = FxHashMap::default();

        for cn in query.corpus_names {
            let prep =
                self.prepare_query(cn.as_ref(), query.query, query.query_language, |_| vec![])?;

            // acquire read-only lock and execute query
            let lock = prep.db_entry.read().unwrap();
            let db: &AnnotationGraph = get_read_or_error(&lock)?;

            // get the matching annotation keys for each definition entry
            let mut annokeys: Vec<(usize, Vec<AnnoKey>)> = Vec::default();
            for def in definition.iter() {
                if let Some(node_ref) = prep.query.get_variable_pos(&def.node_ref) {
                    if let Some(ns) = &def.ns {
                        // add the single fully qualified annotation key
                        annokeys.push((
                            node_ref,
                            vec![AnnoKey {
                                ns: ns.clone().into(),
                                name: def.name.clone().into(),
                            }],
                        ));
                    } else {
                        // add all matching annotation keys
                        annokeys.push((node_ref, db.get_node_annos().get_qnames(&def.name)));
                    }
                }
            }

            let plan = ExecutionPlan::from_disjunction(&prep.query, &db, &self.query_config)?;

            for mgroup in plan {
                // for each match, extract the defined annotation (by its key) from the result node
                let mut tuple: Vec<String> = Vec::with_capacity(annokeys.len());
                for (node_ref, anno_keys) in &annokeys {
                    let mut tuple_val: String = String::default();
                    if *node_ref < mgroup.len() {
                        let m: &Match = &mgroup[*node_ref];
                        for k in anno_keys.iter() {
                            if let Some(val) = db.get_node_annos().get_value_for_item(&m.node, k) {
                                tuple_val = val.to_string();
                            }
                        }
                    }
                    tuple.push(tuple_val);
                }
                // add the tuple to the frequency count
                let tuple_count: &mut usize = tuple_frequency.entry(tuple).or_insert(0);
                *tuple_count += 1;

                if *tuple_count % 1_000 == 0 {
                    timeout.check()?;
                }
            }
        }

        // output the frequency
        let mut result: FrequencyTable<String> = FrequencyTable::default();
        for (tuple, count) in tuple_frequency {
            result.push(FrequencyTableRow {
                values: tuple,
                count,
            });
        }

        // sort the output (largest to smallest)
        result.sort_by(|a, b| a.count.cmp(&b.count).reverse());

        Ok(result)
    }

    /// Parses a `query`and return a list of descriptions for its nodes.
    ///
    /// - `query` - The query to be analyzed.
    /// - `query_language` - The query language of the query (e.g. AQL).
    pub fn node_descriptions(
        &self,
        query: &str,
        query_language: QueryLanguage,
    ) -> Result<Vec<QueryAttributeDescription>> {
        let mut result = Vec::new();
        // parse query
        let q: Disjunction = match query_language {
            QueryLanguage::AQL => aql::parse(query, false)?,
            QueryLanguage::AQLQuirksV3 => aql::parse(query, true)?,
        };

        for (component_nr, alt) in q.alternatives.iter().enumerate() {
            for mut n in alt.get_node_descriptions() {
                n.alternative = component_nr;
                result.push(n);
            }
        }

        Ok(result)
    }

    /// Returns a list of all components of a corpus given by `corpus_name`.
    ///
    /// - `ctype` - Optionally filter by the component type.
    /// - `name` - Optionally filter by the component name.
    pub fn list_components(
        &self,
        corpus_name: &str,
        ctype: Option<AnnotationComponentType>,
        name: Option<&str>,
    ) -> Vec<Component<AnnotationComponentType>> {
        if let Ok(db_entry) = self.get_loaded_entry(corpus_name, false) {
            let lock = db_entry.read().unwrap();
            if let Ok(db) = get_read_or_error(&lock) {
                return db.get_all_components(ctype, name);
            }
        }
        return vec![];
    }

    /// Returns a list of all node annotations of a corpus given by `corpus_name`.
    ///
    /// - `list_values` - If true include the possible values in the result.
    /// - `only_most_frequent_values` - If both this argument and `list_values` are true, only return the most frequent value for each annotation name.
    pub fn list_node_annotations(
        &self,
        corpus_name: &str,
        list_values: bool,
        only_most_frequent_values: bool,
    ) -> Vec<Annotation> {
        let mut result: Vec<Annotation> = Vec::new();
        if let Ok(db_entry) = self.get_loaded_entry(corpus_name, false) {
            let lock = db_entry.read().unwrap();
            if let Ok(db) = get_read_or_error(&lock) {
                let node_annos: &dyn AnnotationStorage<NodeID> = db.get_node_annos();
                for key in node_annos.annotation_keys() {
                    if list_values {
                        if only_most_frequent_values {
                            // get the first value
                            if let Some(val) =
                                node_annos.get_all_values(&key, true).into_iter().next()
                            {
                                result.push(Annotation {
                                    key: key.clone(),
                                    val: val.into(),
                                });
                            }
                        } else {
                            // get all values
                            for val in node_annos.get_all_values(&key, false) {
                                result.push(Annotation {
                                    key: key.clone(),
                                    val: val.into(),
                                });
                            }
                        }
                    } else {
                        result.push(Annotation {
                            key: key.clone(),
                            val: SmartString::default(),
                        });
                    }
                }
            }
        }

        result
    }

    /// Returns a list of all edge annotations of a corpus given by `corpus_name` and the `component`.
    ///
    /// - `list_values` - If true include the possible values in the result.
    /// - `only_most_frequent_values` - If both this argument and `list_values` are true, only return the most frequent value for each annotation name.
    pub fn list_edge_annotations(
        &self,
        corpus_name: &str,
        component: &Component<AnnotationComponentType>,
        list_values: bool,
        only_most_frequent_values: bool,
    ) -> Vec<Annotation> {
        let mut result: Vec<Annotation> = Vec::new();
        if let Ok(db_entry) =
            self.get_loaded_entry_with_components(corpus_name, vec![component.clone()])
        {
            let lock = db_entry.read().unwrap();
            if let Ok(db) = get_read_or_error(&lock) {
                if let Some(gs) = db.get_graphstorage(&component) {
                    let edge_annos = gs.get_anno_storage();
                    for key in edge_annos.annotation_keys() {
                        if list_values {
                            if only_most_frequent_values {
                                // get the first value
                                if let Some(val) =
                                    edge_annos.get_all_values(&key, true).into_iter().next()
                                {
                                    result.push(Annotation {
                                        key: key.clone(),
                                        val: val.into(),
                                    });
                                }
                            } else {
                                // get all values
                                for val in edge_annos.get_all_values(&key, false) {
                                    result.push(Annotation {
                                        key: key.clone(),
                                        val: val.into(),
                                    });
                                }
                            }
                        } else {
                            result.push(Annotation {
                                key: key.clone(),
                                val: SmartString::new(),
                            });
                        }
                    }
                }
            }
        }

        result
    }

    fn check_cache_size_and_remove(&self, keep: Vec<&str>, report_cache_status: bool) {
        let mut cache_lock = self.corpus_cache.write().unwrap();
        let cache = &mut *cache_lock;
        check_cache_size_and_remove_with_cache(
            cache,
            &self.cache_strategy,
            keep,
            report_cache_status,
        );
    }
}

impl Drop for CorpusStorage {
    fn drop(&mut self) {
        // wait until all background workers are finished
        let &(ref lock, ref cvar) = &*self.active_background_workers;
        let mut nr_active_background_workers = lock.lock().unwrap();
        while *nr_active_background_workers > 0 {
            trace!(
                "Waiting for background thread to finish ({} worker(s) left)...",
                *nr_active_background_workers
            );
            nr_active_background_workers = cvar.wait(nr_active_background_workers).unwrap();
        }

        // unlock lock file
        if let Err(e) = self.lock_file.unlock() {
            warn!("Could not unlock CorpusStorage lock file: {:?}", e);
        } else {
            trace!("Unlocked CorpusStorage lock file");
        }
    }
}

fn get_read_or_error<'a>(lock: &'a RwLockReadGuard<CacheEntry>) -> Result<&'a AnnotationGraph> {
    if let CacheEntry::Loaded(ref db) = &**lock {
        Ok(db)
    } else {
        Err(GraphAnnisError::LoadingGraphFailed {
            name: "".to_string(),
        })
    }
}

fn get_write_or_error<'a>(
    lock: &'a mut RwLockWriteGuard<CacheEntry>,
) -> Result<&'a mut AnnotationGraph> {
    if let CacheEntry::Loaded(ref mut db) = &mut **lock {
        Ok(db)
    } else {
        Err(CorpusStorageError::CorpusCacheEntryNotLoaded.into())
    }
}

fn get_cache_sizes(
    cache: &LinkedHashMap<String, Arc<RwLock<CacheEntry>>>,
) -> LinkedHashMap<String, usize> {
    let mut mem_ops = MallocSizeOfOps::new(memory_estimation::platform::usable_size, None, None);

    let mut db_sizes: LinkedHashMap<String, usize> = LinkedHashMap::new();
    for (corpus, db_entry) in cache.iter() {
        let lock = db_entry.read().unwrap();
        if let CacheEntry::Loaded(ref db) = &*lock {
            let s = db.size_of_cached(&mut mem_ops);
            db_sizes.insert(corpus.clone(), s);
        }
    }
    db_sizes
}

fn get_max_cache_size(cache_strategy: &CacheStrategy, used_cache_size: usize) -> usize {
    match cache_strategy {
        CacheStrategy::FixedMaxMemory(max_size) => *max_size * 1_000_000,
        CacheStrategy::PercentOfFreeMemory(max_percent) => {
            // get the current free space in main memory
            if let Ok(mem) = sys_info::mem_info() {
                // the free memory
                let free_system_mem: usize = mem.avail as usize * 1024; // mem.free is in KiB
                                                                        // A part of the system memory is already used by the cache.
                                                                        // We want x percent of the overall available memory (thus not used by us), so add the cache size
                let available_memory: usize = free_system_mem + used_cache_size;
                ((available_memory as f64) * (max_percent / 100.0)) as usize
            } else {
                // fallback to include only the last loaded corpus if free memory size is unknown
                0
            }
        }
    }
}

fn check_cache_size_and_remove_with_cache(
    cache: &mut LinkedHashMap<String, Arc<RwLock<CacheEntry>>>,
    cache_strategy: &CacheStrategy,
    keep: Vec<&str>,
    report_cache_status: bool,
) {
    let keep: HashSet<&str> = keep.into_iter().collect();

    // check size of each corpus and calculate the sum of used memory
    let db_sizes = get_cache_sizes(cache);
    let mut size_sum: usize = db_sizes.iter().map(|(_, s)| s).sum();

    let max_cache_size: usize = get_max_cache_size(cache_strategy, size_sum);

    debug!(
        "Current cache size is {:.2} MB / max  {:.2} MB",
        (size_sum as f64) / 1_000_000.0,
        (max_cache_size as f64) / 1_000_000.0
    );

    // remove older entries (at the beginning) until cache size requirements are met,
    // but never remove the last loaded entry
    for (corpus_name, corpus_size) in db_sizes.iter() {
        if size_sum > max_cache_size {
            if !keep.contains(corpus_name.as_str()) {
                cache.remove(corpus_name);
                size_sum -= corpus_size;
                debug!(
                    "Removing corpus {} from cache. {}",
                    corpus_name,
                    get_corpus_cache_info_as_string(cache, max_cache_size),
                );
            }
        } else {
            // cache size is smaller, nothing to do
            break;
        }
    }

    if report_cache_status {
        info!("{}", get_corpus_cache_info_as_string(cache, max_cache_size));
    }
}

/// Return the current size and loaded corpora as debug string.
fn get_corpus_cache_info_as_string(
    cache: &mut LinkedHashMap<String, Arc<RwLock<CacheEntry>>>,
    max_cache_size: usize,
) -> String {
    let cache_sizes = get_cache_sizes(cache);
    if cache_sizes.is_empty() {
        "Corpus cache is currently empty".to_string()
    } else {
        let corpus_memory_as_string: Vec<String> = cache_sizes
            .iter()
            .map(|(corpus_name, corpus_size)| {
                format!(
                    "{} ({:.2} MB)",
                    corpus_name,
                    (*corpus_size as f64) / 1_000_000.0
                )
            })
            .collect();
        let size_sum: usize = cache_sizes.iter().map(|(_, s)| s).sum();
        format!(
            "Total cache size is {:.2} MB / {:.2} MB and loaded corpora are: {}.",
            (size_sum as f64) / 1_000_000.0,
            (max_cache_size as f64) / 1_000_000.0,
            corpus_memory_as_string.join(", ")
        )
    }
}

fn extract_subgraph_by_query(
    db_entry: &Arc<RwLock<CacheEntry>>,
    query: &Disjunction,
    match_idx: &[usize],
    query_config: &query::Config,
    component_type_filter: Option<AnnotationComponentType>,
) -> Result<AnnotationGraph> {
    // acquire read-only lock and create query that finds the context nodes
    let lock = db_entry.read().unwrap();
    let orig_db = get_read_or_error(&lock)?;

    let plan = ExecutionPlan::from_disjunction(&query, &orig_db, &query_config)?;

    debug!("executing subgraph query\n{}", plan);

    // We have to keep our own unique set because the query will return "duplicates" whenever the other parts of the
    // match vector differ.
    let mut match_result: BTreeSet<Match> = BTreeSet::new();

    let mut result = AnnotationGraph::new(false)?;

    // create the subgraph description
    for r in plan {
        trace!("subgraph query found match {:?}", r);
        for i in match_idx.iter().cloned() {
            if i < r.len() {
                let m: &Match = &r[i];
                if !match_result.contains(m) {
                    match_result.insert(m.clone());
                    trace!("subgraph query extracted node {:?}", m.node);
                    create_subgraph_node(m.node, &mut result, orig_db)?;
                }
            }
        }
    }

    let components = orig_db.get_all_components(component_type_filter, None);

    for m in &match_result {
        create_subgraph_edge(m.node, &mut result, orig_db, &components)?;
    }

    Ok(result)
}

fn create_subgraph_node(
    id: NodeID,
    db: &mut AnnotationGraph,
    orig_db: &AnnotationGraph,
) -> Result<()> {
    // add all node labels with the same node ID
    for a in orig_db.get_node_annos().get_annotations_for_item(&id) {
        db.get_node_annos_mut().insert(id, a)?;
    }
    Ok(())
}
fn create_subgraph_edge(
    source_id: NodeID,
    db: &mut AnnotationGraph,
    orig_db: &AnnotationGraph,
    components: &[Component<AnnotationComponentType>],
) -> Result<()> {
    // find outgoing edges
    for c in components {
        // don't include index components
        let ctype = c.get_type();
        if !((ctype == AnnotationComponentType::Coverage
            && c.layer == "annis"
            && !c.name.is_empty())
            || ctype == AnnotationComponentType::RightToken
            || ctype == AnnotationComponentType::LeftToken)
        {
            if let Some(orig_gs) = orig_db.get_graphstorage(c) {
                for target in orig_gs.get_outgoing_edges(source_id) {
                    if !db
                        .get_node_annos()
                        .get_all_keys_for_item(&target, None, None)
                        .is_empty()
                    {
                        let e = Edge {
                            source: source_id,
                            target,
                        };
                        if let Ok(new_gs) = db.get_or_create_writable(&c) {
                            new_gs.add_edge(e.clone())?;
                        }

                        for a in orig_gs.get_anno_storage().get_annotations_for_item(&Edge {
                            source: source_id,
                            target,
                        }) {
                            if let Ok(new_gs) = db.get_or_create_writable(&c) {
                                new_gs.add_edge_annotation(e.clone(), a)?;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn create_lockfile_for_directory(db_dir: &Path) -> Result<File> {
    std::fs::create_dir_all(&db_dir).map_err(|e| CorpusStorageError::LockCorpusDirectory {
        path: db_dir.to_string_lossy().to_string(),
        source: e,
    })?;
    let lock_file_path = db_dir.join("db.lock");
    // check if we can get the file lock
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(lock_file_path.as_path())
        .map_err(|e| CorpusStorageError::LockCorpusDirectory {
            path: db_dir.to_string_lossy().to_string(),
            source: e,
        })?;
    lock_file
        .try_lock_exclusive()
        .map_err(|e| CorpusStorageError::LockCorpusDirectory {
            path: db_dir.to_string_lossy().to_string(),
            source: e,
        })?;

    Ok(lock_file)
}
