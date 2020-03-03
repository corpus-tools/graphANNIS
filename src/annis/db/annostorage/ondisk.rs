use crate::annis::db::annostorage::AnnotationStorage;
use crate::annis::db::Match;
use crate::annis::db::ValueSearch;
use crate::annis::errors::*;
use crate::annis::types::AnnoKey;
use crate::annis::types::Annotation;
use crate::annis::types::NodeID;
use crate::annis::util;
use crate::annis::util::create_str_vec_key;
use crate::annis::util::disk_collections::{DiskMap, EvictionStrategy, KeySerializer};
use crate::annis::util::memory_estimation;
use crate::annis::util::parse_str_vec_key;
use core::ops::Bound::*;
use rand::seq::IteratorRandom;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SUBFOLDER_NAME: &str = "nodes_diskmap_v1";

const NODE_ID_SIZE: usize = std::mem::size_of::<NodeID>();

/// An on-disk implementation of an annotation storage.
///
/// # Panics
///
/// In contrast to the main-memory implementation, accessing the disk can fail.
/// This is handled as a fatal error with panic except for specific scenarios where we know how to recover from this error.
/// Panics are used because these errors are unrecoverable
/// (e.g. if the file is suddenly missing this is like if someone removed the main memory)
/// and there is no way of delivering a correct answer.
/// Retrying the same query again will also not succeed since temporary errors are already handled internally.
#[derive(MallocSizeOf)]
pub struct AnnoStorageImpl {
    #[ignore_malloc_size_of = "is stored on disk"]
    by_container: DiskMap<Vec<u8>, String>,
    #[ignore_malloc_size_of = "is stored on disk"]
    by_anno_qname: DiskMap<Vec<u8>, bool>,
    #[with_malloc_size_of_func = "memory_estimation::size_of_pathbuf"]
    location: PathBuf,
    /// A handle to a temporary directory. This must be part of the struct because the temporary directory will
    /// be deleted when this handle is dropped.
    #[with_malloc_size_of_func = "memory_estimation::size_of_option_tempdir"]
    temp_dir: Option<tempfile::TempDir>,

    #[with_malloc_size_of_func = "memory_estimation::size_of_btreemap"]
    anno_key_sizes: BTreeMap<AnnoKey, usize>,

    /// additional statistical information
    #[with_malloc_size_of_func = "memory_estimation::size_of_btreemap"]
    histogram_bounds: BTreeMap<AnnoKey, Vec<String>>,
    largest_item: Option<NodeID>,
}

/// Creates a key for the `by_container` tree.
///
/// Structure:
/// ```text
/// [64 Bits Node ID][Namespace]\0[Name]\0
/// ```
fn create_by_container_key(node: NodeID, anno_key: &AnnoKey) -> Vec<u8> {
    let mut result: Vec<u8> = node.create_key();
    result.extend(create_str_vec_key(&[&anno_key.ns, &anno_key.name]));
    result
}

/// Parse the raw data and extract the node ID and the annotation key.
///
/// # Panics
/// Panics if the raw data is smaller than the length of a node ID bit-representation.
fn parse_by_container_key(data: &[u8]) -> (NodeID, AnnoKey) {
    let item = NodeID::parse_key(data);
    let str_vec = parse_str_vec_key(&data[8..]);

    let anno_key = AnnoKey {
        ns: str_vec[0].to_string(),
        name: str_vec[1].to_string(),
    };
    (item, anno_key)
}

/// Creates a key for the `by_anno_qname` tree.
///
/// Since the same (name, ns, value) triple can be used by multiple nodes and we want to avoid
/// arrays as values, the node ID is part of the key and makes it unique.
///
/// Structure:
/// ```text
/// [Namespace]\0[Name]\0[Value]\0[64 Bits Node ID]
/// ```
fn create_by_anno_qname_key(node: NodeID, anno: &Annotation) -> Vec<u8> {
    // Use the qualified annotation name, the value and the node ID as key for the indexes.

    let mut result: Vec<u8> = create_str_vec_key(&[&anno.key.ns, &anno.key.name, &anno.val]);
    result.extend(&node.create_key());
    result
}

/// Parse the raw data and extract the node ID and the annotation.
///
/// # Panics
/// Panics if the raw data is smaller than the length of a node ID bit-representation.
fn parse_by_anno_qname_key(data: &[u8]) -> (NodeID, Annotation) {
    let node_id = NodeID::parse_key(&data[(data.len() - NODE_ID_SIZE)..]);
    let str_vec = parse_str_vec_key(&data[..(data.len() - NODE_ID_SIZE)]);

    let anno = Annotation {
        key: AnnoKey {
            ns: str_vec[0].to_string(),
            name: str_vec[1].to_string(),
        },
        val: str_vec[2].to_string(),
    };

    (node_id, anno)
}

impl AnnoStorageImpl {
    pub fn new(path: Option<PathBuf>) -> Result<AnnoStorageImpl> {
        if let Some(path) = path {
            let path_by_container = path.join("by_container.bin");
            let path_by_anno_qname = path.join("by_anno_qname.bin");

            let mut result = AnnoStorageImpl {
                by_container: DiskMap::new(Some(&path_by_container), EvictionStrategy::default())?,
                by_anno_qname: DiskMap::new(
                    Some(&path_by_anno_qname),
                    EvictionStrategy::default(),
                )?,
                anno_key_sizes: BTreeMap::new(),
                largest_item: None,
                histogram_bounds: BTreeMap::new(),
                location: path.to_path_buf(),
                temp_dir: None,
            };

            // load internal helper fields
            let custom_path = path.join("custom.bin");
            let f = std::fs::File::open(custom_path)?;
            let mut reader = std::io::BufReader::new(f);
            result.largest_item = bincode::deserialize_from(&mut reader)?;
            result.anno_key_sizes = bincode::deserialize_from(&mut reader)?;
            result.histogram_bounds = bincode::deserialize_from(&mut reader)?;

            Ok(result)
        } else {
            let tmp_dir = tempfile::Builder::new()
                .prefix("graphannis-ondisk-nodeanno-")
                .tempdir()?;
            Ok(AnnoStorageImpl {
                by_container: DiskMap::default(),
                by_anno_qname: DiskMap::default(),
                anno_key_sizes: BTreeMap::new(),
                largest_item: None,
                histogram_bounds: BTreeMap::new(),
                location: tmp_dir.as_ref().to_path_buf(),
                temp_dir: Some(tmp_dir),
            })
        }
    }

    fn matching_items<'a>(
        &'a self,
        namespace: Option<&str>,
        name: &str,
        value: Option<&str>,
    ) -> Box<dyn Iterator<Item = (NodeID, Arc<AnnoKey>)> + 'a> {
        let key_ranges: Vec<Arc<AnnoKey>> = if let Some(ns) = namespace {
            vec![Arc::from(AnnoKey {
                ns: ns.to_string(),
                name: name.to_string(),
            })]
        } else {
            self.get_qnames(name)
                .into_iter()
                .map(|key| Arc::from(key))
                .collect()
        };

        let value = value.map(|v| v.to_string());

        let it = key_ranges
            .into_iter()
            .flat_map(move |anno_key| {
                let lower_bound = create_by_anno_qname_key(
                    NodeID::min_value(),
                    &Annotation {
                        key: anno_key.as_ref().clone(),
                        val: if let Some(value) = &value {
                            value.to_string()
                        } else {
                            "".to_string()
                        },
                    },
                );
                let upper_bound = create_by_anno_qname_key(
                    NodeID::max_value(),
                    &Annotation {
                        key: anno_key.as_ref().clone(),
                        val: if let Some(value) = &value {
                            value.to_string()
                        } else {
                            std::char::MAX.to_string()
                        },
                    },
                );
                self.by_anno_qname.range(lower_bound..upper_bound)
            })
            .fuse()
            .map(|(data, _)| {
                let parsed = parse_by_anno_qname_key(&data);
                (parsed.0, Arc::from(parsed.1.key))
            });

        Box::new(it)
    }

    fn get_by_anno_qname_range<'a>(
        &'a self,
        anno_key: &AnnoKey,
    ) -> Box<dyn Iterator<Item = (Vec<u8>, bool)> + 'a> {
        let lower_bound = create_by_anno_qname_key(
            NodeID::min_value(),
            &Annotation {
                key: anno_key.clone(),
                val: "".to_string(),
            },
        );
        let upper_bound = create_by_anno_qname_key(
            NodeID::max_value(),
            &Annotation {
                key: anno_key.clone(),
                val: std::char::MAX.to_string(),
            },
        );

        Box::new(self.by_anno_qname.range(lower_bound..upper_bound))
    }
}

impl<'de> AnnotationStorage<NodeID> for AnnoStorageImpl {
    fn insert(&mut self, item: NodeID, anno: Annotation) -> Result<()> {
        // insert the value into main tree
        let by_container_key = create_by_container_key(item, &anno.key);

        let already_existed = self.by_container.try_get(&by_container_key)?.is_some();
        self.by_container
            .insert(by_container_key, anno.val.clone())?;

        // To save some space, insert an empty array as a marker value
        // (all information is part of the key already)
        self.by_anno_qname
            .insert(create_by_anno_qname_key(item, &anno), true)?;

        if !already_existed {
            // a new annotation entry was inserted and did not replace an existing one
            if let Some(largest_item) = self.largest_item.clone() {
                if largest_item < item {
                    self.largest_item = Some(item);
                }
            } else {
                self.largest_item = Some(item);
            }

            let anno_key_entry = self.anno_key_sizes.entry(anno.key.clone()).or_insert(0);
            *anno_key_entry += 1;
        }

        Ok(())
    }

    fn get_annotations_for_item(&self, item: &NodeID) -> Vec<Annotation> {
        let mut result = Vec::default();
        for (key, val) in self
            .by_container
            .range(item.create_key()..(*item + 1).create_key())
        {
            let parsed_key = parse_by_container_key(&key);
            let anno = Annotation {
                key: parsed_key.1,
                val: val,
            };
            result.push(anno);
        }

        result
    }

    fn remove_annotation_for_item(
        &mut self,
        item: &NodeID,
        key: &AnnoKey,
    ) -> Result<Option<Cow<str>>> {
        // remove annotation from by_container
        let by_container_key = create_by_container_key(*item, key);
        if let Some(val) = self.by_container.remove(&by_container_key)? {
            // remove annotation from by_anno_qname
            let anno = Annotation {
                key: key.clone(),
                val,
            };

            self.by_anno_qname
                .remove(&create_by_anno_qname_key(*item, &anno))?;
            // decrease the annotation count for this key
            let new_key_count: usize = if let Some(num_of_keys) = self.anno_key_sizes.get_mut(key) {
                *num_of_keys -= 1;
                *num_of_keys
            } else {
                0
            };
            // if annotation count dropped to zero remove the key
            if new_key_count == 0 {
                self.anno_key_sizes.remove(key);
            }

            Ok(Some(Cow::Owned(anno.val)))
        } else {
            Ok(None)
        }
    }

    fn clear(&mut self) -> Result<()> {
        self.by_container.clear();
        self.by_anno_qname.clear();

        self.largest_item = None;
        self.anno_key_sizes.clear();
        self.histogram_bounds.clear();

        Ok(())
    }

    fn get_qnames(&self, name: &str) -> Vec<AnnoKey> {
        let it = self.anno_key_sizes.range(
            AnnoKey {
                name: name.to_owned(),
                ns: String::default(),
            }..,
        );
        let mut result: Vec<AnnoKey> = Vec::default();
        for (k, _) in it {
            if k.name == name {
                result.push(k.clone());
            } else {
                break;
            }
        }
        result
    }

    fn number_of_annotations(&self) -> usize {
        self.by_container.iter().count()
    }

    fn get_value_for_item(&self, item: &NodeID, key: &AnnoKey) -> Option<Cow<str>> {
        let raw = self.by_container.get(&create_by_container_key(*item, key));
        if let Some(val) = raw {
            Some(Cow::Owned(val))
        } else {
            None
        }
    }

    fn get_keys_for_iterator(
        &self,
        ns: Option<&str>,
        name: Option<&str>,
        it: Box<dyn Iterator<Item = NodeID>>,
    ) -> Vec<Match> {
        if let Some(name) = name {
            if let Some(ns) = ns {
                // return the only possible annotation for each node
                let key = Arc::from(AnnoKey {
                    ns: ns.to_string(),
                    name: name.to_string(),
                });
                let mut matches: Vec<Match> = Vec::new();
                // createa a template key
                let mut container_key = create_by_container_key(0, &key);
                for item in it {
                    // Set the first bytes to the ID of the item.
                    // This saves the repeated expensive construction of the annotation key part.
                    container_key[0..NODE_ID_SIZE][0..NODE_ID_SIZE]
                        .copy_from_slice(&item.to_be_bytes());
                    if self.by_container.get(&container_key).is_some() {
                        matches.push((item, key.clone()).into());
                    }
                }
                matches
            } else {
                let mut matching_qnames: Vec<(Vec<u8>, Arc<AnnoKey>)> = self
                    .get_qnames(&name)
                    .into_iter()
                    .map(|key| (create_by_container_key(0, &key), Arc::from(key)))
                    .collect();
                // return all annotations with the correct name for each node
                let mut matches: Vec<Match> = Vec::new();
                for item in it {
                    for (container_key, anno_key) in matching_qnames.iter_mut() {
                        // Set the first bytes to the ID of the item.
                        // This saves the repeated expensive construction of the annotation key part.
                        container_key[0..NODE_ID_SIZE][0..NODE_ID_SIZE]
                            .copy_from_slice(&item.to_be_bytes());
                        if self.by_container.get(container_key).is_some() {
                            matches.push((item, anno_key.clone()).into());
                        }
                    }
                }
                matches
            }
        } else {
            // get all annotation keys for this item
            it.flat_map(|item| {
                let prefix = item.create_key();
                let mut after_prefix = Vec::with_capacity(prefix.len());
                if let Some(last) = after_prefix.last_mut() {
                    *last = *last + 1;
                }
                self.by_container
                    .range(prefix..after_prefix)
                    .map(|(data, _)| {
                        let (node, matched_anno_key) = parse_by_container_key(&data);
                        Match {
                            node,
                            anno_key: Arc::from(matched_anno_key),
                        }
                    })
            })
            .collect()
        }
    }

    fn number_of_annotations_by_name(&self, ns: Option<&str>, name: &str) -> usize {
        let qualified_keys = match ns {
            Some(ns) => self.anno_key_sizes.range((
                Included(AnnoKey {
                    name: name.to_string(),
                    ns: ns.to_string(),
                }),
                Included(AnnoKey {
                    name: name.to_string(),
                    ns: ns.to_string(),
                }),
            )),
            None => self.anno_key_sizes.range(
                AnnoKey {
                    name: name.to_string(),
                    ns: String::default(),
                }..AnnoKey {
                    name: name.to_string(),
                    ns: std::char::MAX.to_string(),
                },
            ),
        };
        let mut result = 0;
        for (_anno_key, anno_size) in qualified_keys {
            result += anno_size;
        }
        result
    }

    fn exact_anno_search<'a>(
        &'a self,
        namespace: Option<&str>,
        name: &str,
        value: ValueSearch<&str>,
    ) -> Box<dyn Iterator<Item = Match> + 'a> {
        match value {
            ValueSearch::Any => {
                let it = self
                    .matching_items(namespace, name, None)
                    .map(move |item| item.into());
                Box::new(it)
            }
            ValueSearch::Some(value) => {
                let it = self
                    .matching_items(namespace, name, Some(value))
                    .map(move |item| item.into());
                Box::new(it)
            }
            ValueSearch::NotSome(value) => {
                let value = value.to_string();
                let it = self
                    .matching_items(namespace, name, None)
                    .filter(move |(node, anno_key)| {
                        if let Some(item_value) = self.get_value_for_item(node, anno_key) {
                            item_value != value
                        } else {
                            false
                        }
                    })
                    .map(move |item| item.into());
                Box::new(it)
            }
        }
    }

    fn regex_anno_search<'a>(
        &'a self,
        namespace: Option<&str>,
        name: &str,
        pattern: &str,
        negated: bool,
    ) -> Box<dyn Iterator<Item = Match> + 'a> {
        let full_match_pattern = util::regex_full_match(pattern);
        let compiled_result = regex::Regex::new(&full_match_pattern);
        if let Ok(re) = compiled_result {
            let it = self
                .matching_items(namespace, name, None)
                .filter(move |(node, anno_key)| {
                    if let Some(val) = self.get_value_for_item(node, anno_key) {
                        if negated {
                            !re.is_match(&val)
                        } else {
                            re.is_match(&val)
                        }
                    } else {
                        false
                    }
                })
                .map(move |item| item.into());
            return Box::new(it);
        } else if negated {
            // return all values
            return self.exact_anno_search(namespace, name, None.into());
        } else {
            // if regular expression pattern is invalid return empty iterator
            return Box::new(std::iter::empty());
        }
    }

    fn get_all_keys_for_item(
        &self,
        item: &NodeID,
        ns: Option<&str>,
        name: Option<&str>,
    ) -> Vec<Arc<AnnoKey>> {
        if let Some(name) = name {
            if let Some(ns) = ns {
                let key = Arc::from(AnnoKey {
                    ns: ns.to_string(),
                    name: name.to_string(),
                });
                if self
                    .by_container
                    .get(&create_by_container_key(*item, &key))
                    .is_some()
                {
                    return vec![key.clone()];
                }
                vec![]
            } else {
                // get all qualified names for the given annotation name
                let res: Vec<Arc<AnnoKey>> = self
                    .get_qnames(&name)
                    .into_iter()
                    .filter(|key| {
                        self.by_container
                            .get(&create_by_container_key(*item, key))
                            .is_some()
                    })
                    .map(|key| Arc::from(key))
                    .collect();
                res
            }
        } else {
            // no annotation name given, return all
            self.get_annotations_for_item(item)
                .into_iter()
                .map(|anno| Arc::from(anno.key))
                .collect()
        }
    }

    fn guess_max_count(
        &self,
        ns: Option<&str>,
        name: &str,
        lower_val: &str,
        upper_val: &str,
    ) -> usize {
        // find all complete keys which have the given name (and namespace if given)
        let qualified_keys = match ns {
            Some(ns) => vec![AnnoKey {
                name: name.to_string(),
                ns: ns.to_string(),
            }],
            None => self.get_qnames(&name),
        };

        let mut universe_size: usize = 0;
        let mut sum_histogram_buckets: usize = 0;
        let mut count_matches: usize = 0;

        // guess for each fully qualified annotation key and return the sum of all guesses
        for anno_key in qualified_keys {
            if let Some(anno_size) = self.anno_key_sizes.get(&anno_key) {
                universe_size += *anno_size;

                if let Some(histo) = self.histogram_bounds.get(&anno_key) {
                    // find the range in which the value is contained

                    // we need to make sure the histogram is not empty -> should have at least two bounds
                    if histo.len() >= 2 {
                        sum_histogram_buckets += histo.len() - 1;

                        for i in 0..histo.len() - 1 {
                            let bucket_begin = &histo[i];
                            let bucket_end = &histo[i + 1];
                            // check if the range overlaps with the search range
                            if bucket_begin.as_str() <= upper_val
                                && lower_val <= bucket_end.as_str()
                            {
                                count_matches += 1;
                            }
                        }
                    }
                }
            }
        }

        if sum_histogram_buckets > 0 {
            let selectivity: f64 = (count_matches as f64) / (sum_histogram_buckets as f64);
            (selectivity * (universe_size as f64)).round() as usize
        } else {
            0
        }
    }

    fn guess_max_count_regex(&self, ns: Option<&str>, name: &str, pattern: &str) -> usize {
        let full_match_pattern = util::regex_full_match(pattern);

        let parsed = regex_syntax::Parser::new().parse(&full_match_pattern);
        if let Ok(parsed) = parsed {
            let expr: regex_syntax::hir::Hir = parsed;

            let prefix_set = regex_syntax::hir::literal::Literals::prefixes(&expr);
            let val_prefix = std::str::from_utf8(prefix_set.longest_common_prefix());

            if let Ok(lower_val) = val_prefix {
                let mut upper_val = String::from(lower_val);
                upper_val.push(std::char::MAX);
                return self.guess_max_count(ns, name, &lower_val, &upper_val);
            }
        }

        0
    }

    fn guess_most_frequent_value(&self, ns: Option<&str>, name: &str) -> Option<Cow<str>> {
        // find all complete keys which have the given name (and namespace if given)
        let qualified_keys = match ns {
            Some(ns) => vec![AnnoKey {
                name: name.to_string(),
                ns: ns.to_string(),
            }],
            None => self.get_qnames(&name),
        };

        let mut sampled_values: HashMap<&str, usize> = HashMap::default();

        // guess for each fully qualified annotation key
        for anno_key in qualified_keys {
            if let Some(histo) = self.histogram_bounds.get(&anno_key) {
                for v in histo.iter() {
                    let count: &mut usize = sampled_values.entry(v).or_insert(0);
                    *count += 1;
                }
            }
        }
        // find the value which is most frequent
        if !sampled_values.is_empty() {
            let mut max_count = 0;
            let mut max_value = Cow::Borrowed("");
            for (v, count) in sampled_values.into_iter() {
                if count >= max_count {
                    max_value = Cow::Borrowed(v);
                    max_count = count;
                }
            }
            Some(max_value)
        } else {
            None
        }
    }

    fn get_all_values(&self, key: &AnnoKey, most_frequent_first: bool) -> Vec<Cow<str>> {
        if most_frequent_first {
            let mut values_with_count: HashMap<String, usize> = HashMap::default();
            for (data, _) in self.get_by_anno_qname_range(key) {
                let (_, anno) = parse_by_anno_qname_key(&data);
                let val = anno.val;

                let count = values_with_count.entry(val).or_insert(0);
                *count += 1;
            }
            let mut values_with_count: Vec<(usize, Cow<str>)> = values_with_count
                .into_iter()
                .map(|(val, count)| (count, Cow::Owned(val)))
                .collect();
            values_with_count.sort();
            return values_with_count
                .into_iter()
                .map(|(_count, val)| val)
                .collect();
        } else {
            let values_unique: HashSet<Cow<str>> = self
                .get_by_anno_qname_range(key)
                .map(|(data, _)| {
                    let (_, anno) = parse_by_anno_qname_key(&data);
                    Cow::Owned(anno.val)
                })
                .collect();
            return values_unique.into_iter().collect();
        }
    }

    fn annotation_keys(&self) -> Vec<AnnoKey> {
        self.anno_key_sizes.keys().cloned().collect()
    }

    fn get_largest_item(&self) -> Option<NodeID> {
        self.largest_item.clone()
    }

    fn calculate_statistics(&mut self) {
        let max_histogram_buckets = 250;
        let max_sampled_annotations = 2500;

        self.histogram_bounds.clear();

        // collect statistics for each annotation key separately
        for anno_key in self.anno_key_sizes.keys() {
            // sample a maximal number of annotation values
            let mut rng = rand::thread_rng();

            let all_values_for_key = self.get_by_anno_qname_range(anno_key);

            let mut sampled_anno_values: Vec<String> = all_values_for_key
                .choose_multiple(&mut rng, max_sampled_annotations)
                .into_iter()
                .map(|data| {
                    let (data, _) = data;
                    let (_, anno) = parse_by_anno_qname_key(&data);
                    anno.val
                })
                .collect();

            // create uniformly distributed histogram bounds
            sampled_anno_values.sort();

            let num_hist_bounds = if sampled_anno_values.len() < (max_histogram_buckets + 1) {
                sampled_anno_values.len()
            } else {
                max_histogram_buckets + 1
            };

            let hist = self
                .histogram_bounds
                .entry(anno_key.clone())
                .or_insert_with(std::vec::Vec::new);

            if num_hist_bounds >= 2 {
                hist.resize(num_hist_bounds, String::from(""));

                let delta: usize = (sampled_anno_values.len() - 1) / (num_hist_bounds - 1);
                let delta_fraction: usize = (sampled_anno_values.len() - 1) % (num_hist_bounds - 1);

                let mut pos = 0;
                let mut pos_fraction = 0;
                for hist_item in hist.iter_mut() {
                    *hist_item = sampled_anno_values[pos].clone();
                    pos += delta;
                    pos_fraction += delta_fraction;

                    if pos_fraction >= (num_hist_bounds - 1) {
                        pos += 1;
                        pos_fraction -= num_hist_bounds - 1;
                    }
                }
            }
        }
    }

    fn load_annotations_from(&mut self, location: &Path) -> Result<()> {
        let location = location.join(SUBFOLDER_NAME);

        if !self.location.eq(&location) {
            self.by_container = DiskMap::new(
                Some(&location.join("by_container.bin")),
                EvictionStrategy::default(),
            )?;
            self.by_anno_qname = DiskMap::new(
                Some(&location.join("by_anno_qname.bin")),
                EvictionStrategy::default(),
            )?;
        }

        // load internal helper fields
        let f = std::fs::File::open(location.join("custom.bin"))?;
        let mut reader = std::io::BufReader::new(f);
        self.largest_item = bincode::deserialize_from(&mut reader)?;
        self.anno_key_sizes = bincode::deserialize_from(&mut reader)?;
        self.histogram_bounds = bincode::deserialize_from(&mut reader)?;

        Ok(())
    }

    fn save_annotations_to(&self, location: &Path) -> Result<()> {
        let location = location.join(SUBFOLDER_NAME);

        // save the data
        if self.location.eq(&location) {
            unimplemented!()
        } else {
            // open disk maps for the given location and export to them
            let mut export_by_container = DiskMap::new(
                Some(&location.join("by_container.bin")),
                EvictionStrategy::default(),
            )?;
            for (k, v) in self.by_container.try_iter()? {
                export_by_container.insert(k, v)?;
            }
            export_by_container.compact_and_flush()?;

            let mut export_by_anno_qname = DiskMap::new(
                Some(&location.join("by_anno_qname.bin")),
                EvictionStrategy::default(),
            )?;
            for (k, v) in self.by_container.try_iter()? {
                export_by_anno_qname.insert(k, v)?;
            }
            export_by_anno_qname.compact_and_flush()?;
        }

        // save the other custom fields
        let f = std::fs::File::create(location.join("custom.bin"))?;
        let mut writer = std::io::BufWriter::new(f);
        bincode::serialize_into(&mut writer, &self.largest_item)?;
        bincode::serialize_into(&mut writer, &self.anno_key_sizes)?;
        bincode::serialize_into(&mut writer, &self.histogram_bounds)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Once;
    static LOGGER_INIT: Once = Once::new();

    #[test]
    fn insert_same_anno() {
        LOGGER_INIT.call_once(|| env_logger::init());

        let test_anno = Annotation {
            key: AnnoKey {
                name: "anno1".to_owned(),
                ns: "annis".to_owned(),
            },
            val: "test".to_owned(),
        };

        let mut a = AnnoStorageImpl::new(None).unwrap();

        debug!("Inserting annotation for node 1");
        a.insert(1, test_anno.clone()).unwrap();
        debug!("Inserting annotation for node 1 (again)");
        a.insert(1, test_anno.clone()).unwrap();
        debug!("Inserting annotation for node 2");
        a.insert(2, test_anno.clone()).unwrap();
        debug!("Inserting annotation for node 3");
        a.insert(3, test_anno).unwrap();

        assert_eq!(3, a.number_of_annotations());

        assert_eq!(
            "test",
            a.get_value_for_item(
                &3,
                &AnnoKey {
                    name: "anno1".to_owned(),
                    ns: "annis".to_owned()
                }
            )
            .unwrap()
        );
    }

    #[test]
    fn get_all_for_node() {
        LOGGER_INIT.call_once(|| env_logger::init());

        let test_anno1 = Annotation {
            key: AnnoKey {
                name: "anno1".to_owned(),
                ns: "annis1".to_owned(),
            },
            val: "test".to_owned(),
        };
        let test_anno2 = Annotation {
            key: AnnoKey {
                name: "anno2".to_owned(),
                ns: "annis2".to_owned(),
            },
            val: "test".to_owned(),
        };
        let test_anno3 = Annotation {
            key: AnnoKey {
                name: "anno3".to_owned(),
                ns: "annis1".to_owned(),
            },
            val: "test".to_owned(),
        };

        let mut a = AnnoStorageImpl::new(None).unwrap();

        a.insert(1, test_anno1.clone()).unwrap();
        a.insert(1, test_anno2.clone()).unwrap();
        a.insert(1, test_anno3.clone()).unwrap();

        assert_eq!(3, a.number_of_annotations());

        let all = a.get_annotations_for_item(&1);
        assert_eq!(3, all.len());

        assert_eq!(test_anno1, all[0]);
        assert_eq!(test_anno3, all[1]);
        assert_eq!(test_anno2, all[2]);
    }

    #[test]
    fn remove() {
        LOGGER_INIT.call_once(|| env_logger::init());
        let test_anno = Annotation {
            key: AnnoKey {
                name: "anno1".to_owned(),
                ns: "annis1".to_owned(),
            },
            val: "test".to_owned(),
        };

        let mut a = AnnoStorageImpl::new(None).unwrap();
        a.insert(1, test_anno.clone()).unwrap();

        assert_eq!(1, a.number_of_annotations());
        assert_eq!(1, a.anno_key_sizes.len());
        assert_eq!(&1, a.anno_key_sizes.get(&test_anno.key).unwrap());

        a.remove_annotation_for_item(&1, &test_anno.key).unwrap();

        assert_eq!(0, a.number_of_annotations());
        assert_eq!(&0, a.anno_key_sizes.get(&test_anno.key).unwrap_or(&0));
    }
}
