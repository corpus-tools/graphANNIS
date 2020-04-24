pub mod storage;
pub mod update;

use crate::types::{AnnoKey, Annotation, Component, ComponentType, Edge, NodeID};
use crate::{
    annostorage::AnnotationStorage,
    graph::storage::{registry, GraphStorage, WriteableGraphStorage},
    util::disk_collections::{DiskMap, EvictionStrategy},
};
use anyhow::Result;
use malloc_size_of::{MallocSizeOf, MallocSizeOfOps};
use rayon::prelude::*;
use std;
use std::collections::BTreeMap;
use std::io::prelude::*;
use std::ops::Bound::Included;
use std::path::{Path, PathBuf};
use std::string::ToString;
use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
};
use tempfile;
use update::{GraphUpdate, UpdateEvent};

pub const ANNIS_NS: &str = "annis";
pub const NODE_NAME: &str = "node_name";
pub const NODE_TYPE: &str = "node_type";

lazy_static! {
    pub static ref DEFAULT_ANNO_KEY: Arc<AnnoKey> = Arc::from(AnnoKey::default());
    pub static ref NODE_NAME_KEY: Arc<AnnoKey> = Arc::from(AnnoKey {
        ns: ANNIS_NS.to_owned(),
        name: NODE_NAME.to_owned(),
    });
    /// Return an annotation key which is used for the special `annis::node_type` annotation which every node must have to mark its existance.
    pub static ref NODE_TYPE_KEY: Arc<AnnoKey> = Arc::from(AnnoKey {
        ns: ANNIS_NS.to_owned(),
        name: NODE_TYPE.to_owned(),
    });
}

/// A representation of a graph including node annotations and edges.
/// Edges are partioned into components and each component is implemented by specialized graph storage implementation.
///
/// Use the [CorpusStorage](struct.CorpusStorage.html) struct to create and manage instances of a `Graph`.
///
/// Graphs can have an optional location on the disk.
/// In this case, changes to the graph via the [apply_update(...)](#method.apply_update) function are automatically persisted to this location.
///
pub struct Graph<CT: ComponentType> {
    node_annos: Box<dyn AnnotationStorage<NodeID>>,

    location: Option<PathBuf>,

    components: BTreeMap<Component, Option<Arc<dyn GraphStorage>>>,
    current_change_id: u64,

    background_persistance: Arc<Mutex<()>>,

    cached_size: Mutex<Option<usize>>,

    disk_based: bool,

    phantom: std::marker::PhantomData<CT>,
}

impl<CT: ComponentType> MallocSizeOf for Graph<CT> {
    fn size_of(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut size = self.node_annos.size_of(ops);

        for c in self.components.keys() {
            // TODO: overhead by map is not measured
            size += c.size_of(ops);
            let gs_size = if let Some(gs) = self.get_graphstorage_as_ref(c) {
                gs.size_of(ops) + std::mem::size_of::<usize>()
            } else {
                // Option has the size of the nullable pointer/Arc
                std::mem::size_of::<usize>()
            };
            size += gs_size;
        }

        size
    }
}

fn load_component_from_disk(component_path: Option<PathBuf>) -> Result<Arc<dyn GraphStorage>> {
    let cpath = component_path.ok_or(anyhow!("Can't load component with empty path"))?;

    // load component into memory
    let impl_path = PathBuf::from(&cpath).join("impl.cfg");
    let mut f_impl = std::fs::File::open(impl_path)?;
    let mut impl_name = String::new();
    f_impl.read_to_string(&mut impl_name)?;

    let gs = registry::deserialize(&impl_name, &cpath)?;

    Ok(gs)
}

fn component_to_relative_path(c: &Component) -> PathBuf {
    let mut p = PathBuf::new();
    p.push("gs");
    p.push(c.ctype.to_string());
    p.push(if c.layer.is_empty() {
        "default_layer"
    } else {
        &c.layer
    });
    p.push(&c.name);
    p
}

impl<CT: ComponentType> Graph<CT> {
    /// Create a new and empty instance without any location on the disk.
    pub fn new(disk_based: bool) -> Result<Self> {
        let node_annos: Box<dyn AnnotationStorage<NodeID>> = if disk_based {
            Box::new(crate::annostorage::ondisk::AnnoStorageImpl::new(None)?)
        } else {
            Box::new(crate::annostorage::inmemory::AnnoStorageImpl::<NodeID>::new())
        };

        Ok(Graph {
            node_annos,
            components: BTreeMap::new(),

            location: None,

            current_change_id: 0,

            background_persistance: Arc::new(Mutex::new(())),
            cached_size: Mutex::new(None),

            disk_based,

            phantom: std::marker::PhantomData::<CT>::default(),
        })
    }

    /// Create a new instance without any location on the disk but with the default graph storage components.
    pub fn with_default_graphstorages(disk_based: bool) -> Result<Self> {
        let mut db = Graph::new(disk_based)?;
        for c in CT::default_components() {
            db.get_or_create_writable(&c)?;
        }
        Ok(db)
    }

    fn set_location(&mut self, location: &Path) -> Result<()> {
        self.location = Some(PathBuf::from(location));

        Ok(())
    }

    /// Clear the graph content.
    /// This removes all node annotations, edges and knowledge about components.
    fn clear(&mut self) {
        self.reset_cached_size();
        self.node_annos = Box::new(crate::annostorage::inmemory::AnnoStorageImpl::new());
        self.components.clear();
    }

    /// Load the graph from an external location.
    /// This sets the location of this instance to the given location.
    ///
    /// * `location` - The path on the disk
    /// * `preload` - If `true`, all components are loaded from disk into main memory.
    pub fn load_from(&mut self, location: &Path, preload: bool) -> Result<()> {
        info!("Loading corpus from {}", location.to_string_lossy());
        self.clear();

        let location = PathBuf::from(location);

        self.set_location(location.as_path())?;
        let backup = location.join("backup");

        let mut backup_was_loaded = false;
        let dir2load = if backup.exists() && backup.is_dir() {
            backup_was_loaded = true;
            backup.clone()
        } else {
            location.join("current")
        };

        let ondisk_subdirectory = dir2load.join(crate::annostorage::ondisk::SUBFOLDER_NAME);
        if ondisk_subdirectory.exists() && ondisk_subdirectory.is_dir() {
            self.disk_based = true;
            // directly load the on disk storage from the given folder to avoid having a temporary directory
            let node_annos_tmp =
                crate::annostorage::ondisk::AnnoStorageImpl::new(Some(ondisk_subdirectory))?;
            self.node_annos = Box::new(node_annos_tmp);
        } else {
            // assume a main memory implementation
            self.disk_based = false;
            let mut node_annos_tmp = crate::annostorage::inmemory::AnnoStorageImpl::new();
            node_annos_tmp.load_annotations_from(&dir2load)?;
            self.node_annos = Box::new(node_annos_tmp);
        }

        let log_path = dir2load.join("update_log.bin");

        let logfile_exists = log_path.exists() && log_path.is_file();

        self.find_components_from_disk(&dir2load)?;

        // If backup is active or a write log exists, always  a pre-load to get the complete corpus.
        if preload | logfile_exists | backup_was_loaded {
            self.ensure_loaded_all()?;
        }

        if logfile_exists {
            // apply any outstanding log file updates
            let log_reader = std::fs::File::open(&log_path)?;
            let mut update = bincode::deserialize_from(log_reader)?;
            self.apply_update_in_memory(&mut update, |_| {})?;
        } else {
            self.current_change_id = 0;
        }

        if backup_was_loaded {
            // save the current corpus under the actual location
            self.save_to(&location.join("current"))?;
            // rename backup folder (renaming is atomic and deleting could leave an incomplete backup folder on disk)
            let tmp_dir = tempfile::Builder::new()
                .prefix("temporary-graphannis-backup")
                .tempdir_in(location)?;
            std::fs::rename(&backup, tmp_dir.path())?;
            // remove it after renaming it
            tmp_dir.close()?;
        }

        Ok(())
    }

    fn find_components_from_disk(&mut self, location: &Path) -> Result<()> {
        self.components.clear();

        // for all component types
        for c in CT::all_component_types().into_iter() {
            let cpath = PathBuf::from(location).join("gs").join(c.to_string());

            if cpath.is_dir() {
                // get all the namespaces/layers
                for layer in cpath.read_dir()? {
                    let layer = layer?;
                    if layer.path().is_dir() {
                        // try to load the component with the empty name
                        let empty_name_component = Component {
                            ctype: c.clone().into(),
                            layer: layer.file_name().to_string_lossy().to_string(),
                            name: String::from(""),
                        };
                        {
                            let cfg_file = PathBuf::from(location)
                                .join(component_to_relative_path(&empty_name_component))
                                .join("impl.cfg");

                            if cfg_file.is_file() {
                                self.components.insert(empty_name_component.clone(), None);
                                debug!("Registered component {}", empty_name_component);
                            }
                        }
                        // also load all named components
                        for name in layer.path().read_dir()? {
                            let name = name?;
                            let named_component = Component {
                                ctype: c.clone().into(),
                                layer: layer.file_name().to_string_lossy().to_string(),
                                name: name.file_name().to_string_lossy().to_string(),
                            };
                            let cfg_file = PathBuf::from(location)
                                .join(component_to_relative_path(&named_component))
                                .join("impl.cfg");

                            if cfg_file.is_file() {
                                self.components.insert(named_component.clone(), None);
                                debug!("Registered component {}", named_component);
                            }
                        }
                    }
                }
            }
        } // end for all components
        Ok(())
    }

    fn internal_save(&self, location: &Path) -> Result<()> {
        let location = PathBuf::from(location);

        std::fs::create_dir_all(&location)?;

        self.node_annos.save_annotations_to(&location)?;

        for (c, e) in &self.components {
            if let Some(ref data) = *e {
                let dir = PathBuf::from(&location).join(component_to_relative_path(c));
                std::fs::create_dir_all(&dir)?;

                let impl_name = data.serialization_id();
                data.save_to(&dir)?;

                let cfg_path = PathBuf::from(&dir).join("impl.cfg");
                let mut f_cfg = std::fs::File::create(cfg_path)?;
                f_cfg.write_all(impl_name.as_bytes())?;
            }
        }
        Ok(())
    }

    /// Save the current database to a `location` on the disk, but do not remember this location.
    pub fn save_to(&mut self, location: &Path) -> Result<()> {
        // make sure all components are loaded, otherwise saving them does not make any sense
        self.ensure_loaded_all()?;
        self.internal_save(&location.join("current"))
    }

    /// Save the current database at a new `location` and remember it as new internal location.
    pub fn persist_to(&mut self, location: &Path) -> Result<()> {
        self.set_location(location)?;
        self.internal_save(&location.join("current"))
    }

    fn get_cached_node_id_from_name(
        &self,
        node_name: Cow<String>,
        cache: &mut DiskMap<String, Option<NodeID>>,
    ) -> Result<Option<NodeID>> {
        if let Some(id) = cache.try_get(&node_name)? {
            Ok(id)
        } else {
            let id = self.get_node_id_from_name(&node_name);
            cache.insert(node_name.to_string(), id.clone())?;
            Ok(id)
        }
    }

    #[allow(clippy::cognitive_complexity)]
    fn apply_update_in_memory<F>(&mut self, u: &mut GraphUpdate, progress_callback: F) -> Result<()>
    where
        F: Fn(&str) -> (),
    {
        self.reset_cached_size();

        let all_components = self.get_all_components(None, None);

        let mut update_graph_index = ComponentType::init_graph_update_index(self)?;
        // Cache the expensive mapping of node names to IDs
        let mut node_ids: DiskMap<String, Option<NodeID>> =
            DiskMap::new(None, EvictionStrategy::MaximumItems(1_000_000))?;
        // Iterate once over all changes in the same order as the updates have been added
        let mut nr_updates = 0;
        for (id, change) in u.iter()? {
            trace!("applying event {:?}", &change);
            ComponentType::before_update_event(&change, self, &mut update_graph_index)?;
            match &change {
                UpdateEvent::AddNode {
                    node_name,
                    node_type,
                } => {
                    let existing_node_id =
                        self.get_cached_node_id_from_name(Cow::Borrowed(node_name), &mut node_ids)?;
                    // only add node if it does not exist yet
                    if existing_node_id.is_none() {
                        let new_node_id: NodeID =
                            if let Some(id) = self.node_annos.get_largest_item() {
                                id + 1
                            } else {
                                0
                            };

                        let new_anno_name = Annotation {
                            key: NODE_NAME_KEY.as_ref().clone(),
                            val: node_name.to_string(),
                        };
                        let new_anno_type = Annotation {
                            key: NODE_TYPE_KEY.as_ref().clone(),
                            val: node_type.to_string(),
                        };

                        // add the new node (with minimum labels)
                        self.node_annos.insert(new_node_id, new_anno_name)?;
                        self.node_annos.insert(new_node_id, new_anno_type)?;

                        // update the internal cache
                        node_ids.insert(node_name.clone(), Some(new_node_id))?;
                    }
                }
                UpdateEvent::DeleteNode { node_name } => {
                    if let Some(existing_node_id) =
                        self.get_cached_node_id_from_name(Cow::Borrowed(node_name), &mut node_ids)?
                    {
                        // delete all annotations
                        {
                            for a in self.node_annos.get_annotations_for_item(&existing_node_id) {
                                self.node_annos
                                    .remove_annotation_for_item(&existing_node_id, &a.key)?;
                            }
                        }
                        // delete all edges pointing to this node either as source or target
                        for c in all_components.iter() {
                            if let Ok(gs) = self.get_or_create_writable(c) {
                                gs.delete_node(existing_node_id)?;
                            }
                        }
                    }
                }
                UpdateEvent::AddNodeLabel {
                    node_name,
                    anno_ns,
                    anno_name,
                    anno_value,
                } => {
                    if let Some(existing_node_id) =
                        self.get_cached_node_id_from_name(Cow::Borrowed(node_name), &mut node_ids)?
                    {
                        let anno = Annotation {
                            key: AnnoKey {
                                ns: anno_ns.to_string(),
                                name: anno_name.to_string(),
                            },
                            val: anno_value.to_string(),
                        };
                        self.node_annos.insert(existing_node_id, anno)?;
                    }
                }
                UpdateEvent::DeleteNodeLabel {
                    node_name,
                    anno_ns,
                    anno_name,
                } => {
                    if let Some(existing_node_id) =
                        self.get_cached_node_id_from_name(Cow::Borrowed(node_name), &mut node_ids)?
                    {
                        let key = AnnoKey {
                            ns: anno_ns.to_string(),
                            name: anno_name.to_string(),
                        };
                        self.node_annos
                            .remove_annotation_for_item(&existing_node_id, &key)?;
                    }
                }
                UpdateEvent::AddEdge {
                    source_node,
                    target_node,
                    layer,
                    component_type,
                    component_name,
                } => {
                    let source = self
                        .get_cached_node_id_from_name(Cow::Borrowed(source_node), &mut node_ids)?;
                    let target = self
                        .get_cached_node_id_from_name(Cow::Borrowed(target_node), &mut node_ids)?;
                    // only add edge if both nodes already exist
                    if let (Some(source), Some(target)) = (source, target) {
                        if let Ok(ctype) = CT::from_str(&component_type) {
                            let c = Component {
                                ctype: ctype.into(),
                                layer: layer.to_string(),
                                name: component_name.to_string(),
                            };
                            let gs = self.get_or_create_writable(&c)?;
                            gs.add_edge(Edge { source, target })?;
                        }
                    }
                }
                UpdateEvent::DeleteEdge {
                    source_node,
                    target_node,
                    layer,
                    component_type,
                    component_name,
                } => {
                    let source = self
                        .get_cached_node_id_from_name(Cow::Borrowed(source_node), &mut node_ids)?;
                    let target = self
                        .get_cached_node_id_from_name(Cow::Borrowed(target_node), &mut node_ids)?;
                    if let (Some(source), Some(target)) = (source, target) {
                        if let Ok(ctype) = CT::from_str(&component_type) {
                            let c = Component {
                                ctype: ctype.into(),
                                layer: layer.to_string(),
                                name: component_name.to_string(),
                            };

                            let gs = self.get_or_create_writable(&c)?;
                            gs.delete_edge(&Edge { source, target })?;
                        }
                    }
                }
                UpdateEvent::AddEdgeLabel {
                    source_node,
                    target_node,
                    layer,
                    component_type,
                    component_name,
                    anno_ns,
                    anno_name,
                    anno_value,
                } => {
                    let source = self
                        .get_cached_node_id_from_name(Cow::Borrowed(source_node), &mut node_ids)?;
                    let target = self
                        .get_cached_node_id_from_name(Cow::Borrowed(target_node), &mut node_ids)?;
                    if let (Some(source), Some(target)) = (source, target) {
                        if let Ok(ctype) = CT::from_str(&component_type) {
                            let c = Component {
                                ctype: ctype.into(),
                                layer: layer.to_string(),
                                name: component_name.to_string(),
                            };
                            let gs = self.get_or_create_writable(&c)?;
                            // only add label if the edge already exists
                            let e = Edge { source, target };
                            if gs.is_connected(source, target, 1, Included(1)) {
                                let anno = Annotation {
                                    key: AnnoKey {
                                        ns: anno_ns.to_string(),
                                        name: anno_name.to_string(),
                                    },
                                    val: anno_value.to_string(),
                                };
                                gs.add_edge_annotation(e, anno)?;
                            }
                        }
                    }
                }
                UpdateEvent::DeleteEdgeLabel {
                    source_node,
                    target_node,
                    layer,
                    component_type,
                    component_name,
                    anno_ns,
                    anno_name,
                } => {
                    let source = self
                        .get_cached_node_id_from_name(Cow::Borrowed(source_node), &mut node_ids)?;
                    let target = self
                        .get_cached_node_id_from_name(Cow::Borrowed(target_node), &mut node_ids)?;
                    if let (Some(source), Some(target)) = (source, target) {
                        if let Ok(ctype) = CT::from_str(&component_type) {
                            let c = Component {
                                ctype: ctype.into(),
                                layer: layer.to_string(),
                                name: component_name.to_string(),
                            };
                            let gs = self.get_or_create_writable(&c)?;
                            // only add label if the edge already exists
                            let e = Edge { source, target };
                            if gs.is_connected(source, target, 1, Included(1)) {
                                let key = AnnoKey {
                                    ns: anno_ns.to_string(),
                                    name: anno_name.to_string(),
                                };
                                gs.delete_edge_annotation(&e, &key)?;
                            }
                        }
                    }
                }
            } // end match update entry type
            ComponentType::after_update_event(change, self, &mut update_graph_index)?;
            self.current_change_id = id;

            nr_updates += 1;
            if nr_updates % 100_000 == 0 {
                progress_callback(&format!("applied {} atomic updates", nr_updates));
            }
        } // end for each consistent update entry

        ComponentType::apply_update_graph_index(update_graph_index, self)?;

        Ok(())
    }

    /// Apply a sequence of updates (`u` parameter) to this graph.
    /// If the graph has a location on the disk, the changes are persisted.
    pub fn apply_update<F>(&mut self, u: &mut GraphUpdate, progress_callback: F) -> Result<()>
    where
        F: Fn(&str) -> (),
    {
        progress_callback("applying list of atomic updates");

        // we have to make sure that the corpus is fully loaded (with all components) before we can apply the update.
        self.ensure_loaded_all()?;

        let result = self.apply_update_in_memory(u, &progress_callback);

        progress_callback("memory updates completed, persisting updates to disk");

        if let Some(location) = self.location.clone() {
            trace!("output location for persisting updates is {:?}", location);
            if result.is_ok() {
                let current_path = location.join("current");
                // make sure the output path exits
                std::fs::create_dir_all(&current_path)?;

                // If successfull write log
                let log_path = location.join("update_log.bin");

                // Create a temporary directory in the same file system as the output
                let temporary_dir = tempfile::tempdir_in(&current_path)?;
                let mut temporary_disk_file = tempfile::NamedTempFile::new_in(&temporary_dir)?;

                debug!("writing WAL update log to {:?}", temporary_disk_file.path());
                bincode::serialize_into(temporary_disk_file.as_file(), &u)?;
                temporary_disk_file.flush()?;
                debug!("moving finished WAL update log to {:?}", &log_path);
                // Since the temporary file should be on the same file system, persisting/moving it should be an atomic operation
                temporary_disk_file.persist(&log_path)?;

                progress_callback("finished writing WAL update log");
            } else {
                trace!("error occured while applying updates: {:?}", &result);
                // load corpus from disk again
                self.load_from(&location, true)?;
                return result;
            }
        }

        Ok(())
    }

    /// A function to persist the changes of a write-ahead-log update on the disk. Should be run in a background thread.
    pub fn background_sync_wal_updates(&self) -> Result<()> {
        // TODO: friendly abort any currently running thread

        if let Some(ref location) = self.location {
            // Acquire lock, so that only one thread can write background data at the same time
            let _lock = self.background_persistance.lock().unwrap();

            // Move the old corpus to the backup sub-folder. When the corpus is loaded again and there is backup folder
            // the backup will be used instead of the original possible corrupted files.
            // The current version is only the real one if no backup folder exists. If there is a backup folder
            // there is nothing to do since the backup already contains the last consistent version.
            // A sub-folder is used to ensure that all directories are on the same file system and moving (instead of copying)
            // is possible.
            if !location.join("backup").exists() {
                std::fs::rename(
                    location.join("current"),
                    location.join(location.join("backup")),
                )?;
            }

            // Save the complete corpus without the write log to the target location
            self.internal_save(&location.join("current"))?;

            // remove the backup folder (since the new folder was completly written)
            std::fs::remove_dir_all(location.join("backup"))?;
        }

        Ok(())
    }

    fn component_path(&self, c: &Component) -> Option<PathBuf> {
        match self.location {
            Some(ref loc) => {
                let mut p = PathBuf::from(loc);
                // don't use the backup-folder per default
                p.push("current");
                p.push(component_to_relative_path(c));
                Some(p)
            }
            None => None,
        }
    }

    fn insert_or_copy_writeable(&mut self, c: &Component) -> Result<()> {
        self.reset_cached_size();

        // move the old entry into the ownership of this function
        let entry = self.components.remove(c);
        // component exists?
        if let Some(gs_opt) = entry {
            let mut loaded_comp: Arc<dyn GraphStorage> = if let Some(gs_opt) = gs_opt {
                gs_opt
            } else {
                load_component_from_disk(self.component_path(c))?
            };

            // copy to writable implementation if needed
            let is_writable = {
                Arc::get_mut(&mut loaded_comp)
                    .ok_or_else(|| anyhow!("Could not get mutable reference for component {}", c))?
                    .as_writeable()
                    .is_some()
            };

            let loaded_comp = if is_writable {
                loaded_comp
            } else {
                registry::create_writeable(self, Some(loaded_comp.as_ref()))?
            };

            // (re-)insert the component into map again
            self.components.insert(c.clone(), Some(loaded_comp));
        }
        Ok(())
    }

    /// Makes sure the statistics for the given component are up-to-date.
    pub fn calculate_component_statistics(&mut self, c: &Component) -> Result<()> {
        self.reset_cached_size();

        let mut result: Result<()> = Ok(());
        let mut entry = self
            .components
            .remove(c)
            .ok_or_else(|| anyhow!("Component {} is missing", c.clone()))?;
        if let Some(ref mut gs) = entry {
            if let Some(gs_mut) = Arc::get_mut(gs) {
                // Since immutable graph storages can't change, only writable graph storage statistics need to be re-calculated
                if let Some(writeable_gs) = gs_mut.as_writeable() {
                    writeable_gs.calculate_statistics();
                }
            } else {
                result = Err(anyhow!("Component {} is currently used", c.clone()));
            }
        }
        // re-insert component entry
        self.components.insert(c.clone(), entry);
        result
    }

    /// Gets the the given component.
    /// If the component does not exist yet, it creates a  new empty one.
    /// If the existing component is non-writable, a writable copy of it is created and returned.
    pub fn get_or_create_writable(
        &mut self,
        c: &Component,
    ) -> Result<&mut dyn WriteableGraphStorage> {
        self.reset_cached_size();

        if self.components.contains_key(c) {
            // make sure the component is actually writable and loaded
            self.insert_or_copy_writeable(c)?;
        } else {
            let w = registry::create_writeable(self, None)?;

            self.components.insert(c.clone(), Some(w));
        }

        // get and return the reference to the entry
        let entry: &mut Arc<dyn GraphStorage> = self
            .components
            .get_mut(c)
            .ok_or_else(|| anyhow!("Could not get mutable reference for component {}", c))?
            .as_mut()
            .ok_or_else(|| {
                anyhow!(
                    "Could not get mutable reference to optional value for component {}",
                    c
                )
            })?;
        let gs_mut_ref: &mut dyn GraphStorage = Arc::get_mut(entry)
            .ok_or_else(|| anyhow!("Could not get mutable reference for component {}", c))?;
        Ok(gs_mut_ref.as_writeable().ok_or(anyhow!("Invalid type"))?)
    }

    /// Returns `true` if the graph storage for this specific component is loaded and ready to use.
    pub fn is_loaded(&self, c: &Component) -> bool {
        let entry: Option<&Option<Arc<dyn GraphStorage>>> = self.components.get(c);
        if let Some(gs_opt) = entry {
            if gs_opt.is_some() {
                return true;
            }
        }
        false
    }

    /// Ensure that the graph storages for all component are loaded and ready to use.
    pub fn ensure_loaded_all(&mut self) -> Result<()> {
        let mut components_to_load: Vec<Component> = Vec::with_capacity(self.components.len());

        // colllect all missing components
        for (c, gs) in &self.components {
            if gs.is_none() {
                components_to_load.push(c.clone());
            }
        }

        self.reset_cached_size();

        // load missing components in parallel
        let loaded_components: Vec<(Component, Result<Arc<dyn GraphStorage>>)> = components_to_load
            .into_par_iter()
            .map(|c| {
                info!("Loading component {} from disk", c);
                let cpath = self.component_path(&c);
                let loaded_component = load_component_from_disk(cpath);
                (c, loaded_component)
            })
            .collect();

        // insert all the loaded components
        for (c, gs) in loaded_components {
            let gs = gs?;
            self.components.insert(c, Some(gs));
        }
        Ok(())
    }

    /// Ensure that the graph storage for a specific component is loaded and ready to use.
    pub fn ensure_loaded(&mut self, c: &Component) -> Result<()> {
        // get and return the reference to the entry if loaded
        let entry: Option<Option<Arc<dyn GraphStorage>>> = self.components.remove(c);
        if let Some(gs_opt) = entry {
            let loaded: Arc<dyn GraphStorage> = if let Some(gs_opt) = gs_opt {
                gs_opt
            } else {
                self.reset_cached_size();
                info!("Loading component {} from disk", c);
                load_component_from_disk(self.component_path(c))?
            };

            self.components.insert(c.clone(), Some(loaded));
        }
        Ok(())
    }

    pub fn optimize_impl(&mut self, c: &Component) -> Result<()> {
        if let Some(gs) = self.get_graphstorage(c) {
            if let Some(stats) = gs.get_statistics() {
                let opt_info = registry::get_optimal_impl_heuristic(self, stats);

                // convert if necessary
                if opt_info.id != gs.serialization_id() {
                    let mut new_gs = registry::create_from_info(&opt_info)?;
                    let converted = if let Some(new_gs_mut) = Arc::get_mut(&mut new_gs) {
                        new_gs_mut.copy(self.get_node_annos(), gs.as_ref())?;
                        true
                    } else {
                        false
                    };
                    if converted {
                        self.reset_cached_size();
                        // insert into components map
                        info!(
                            "converted component {} to implementation {}",
                            c, opt_info.id,
                        );
                        self.components.insert(c.clone(), Some(new_gs.clone()));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_node_id_from_name(&self, node_name: &str) -> Option<NodeID> {
        let mut all_nodes_with_anno = self.node_annos.exact_anno_search(
            Some(&ANNIS_NS.to_owned()),
            &NODE_NAME.to_owned(),
            Some(node_name).into(),
        );
        if let Some(m) = all_nodes_with_anno.next() {
            return Some(m.node);
        }
        None
    }

    /// Get a read-only graph storage copy for the given component `c`.
    pub fn get_graphstorage(&self, c: &Component) -> Option<Arc<dyn GraphStorage>> {
        // get and return the reference to the entry if loaded
        let entry: Option<&Option<Arc<dyn GraphStorage>>> = self.components.get(c);
        if let Some(gs_opt) = entry {
            if let Some(ref impl_type) = *gs_opt {
                return Some(impl_type.clone());
            }
        }
        None
    }

    /// Get a read-only graph storage reference for the given component `c`.
    pub fn get_graphstorage_as_ref<'a>(&'a self, c: &Component) -> Option<&'a dyn GraphStorage> {
        // get and return the reference to the entry if loaded
        let entry: Option<&Option<Arc<dyn GraphStorage>>> = self.components.get(c);
        if let Some(gs_opt) = entry {
            if let Some(ref impl_type) = *gs_opt {
                return Some(impl_type.as_ref());
            }
        }
        None
    }

    /// Get a read-only reference to the node annotations of this graph
    pub fn get_node_annos(&self) -> &dyn AnnotationStorage<NodeID> {
        self.node_annos.as_ref()
    }

    /// Get a mutable reference to the node annotations of this graph
    pub fn get_node_annos_mut(&mut self) -> &mut dyn AnnotationStorage<NodeID> {
        self.node_annos.as_mut()
    }

    /// Returns all components of the graph given an optional type (`ctype`) and `name`.
    /// This allows to filter which components to receive.
    /// If you want to retrieve all components, use `None` as value for both arguments.
    pub fn get_all_components(&self, ctype: Option<CT>, name: Option<&str>) -> Vec<Component> {
        if let (Some(ctype), Some(name)) = (&ctype, name) {
            let ctype: u16 = ctype.clone().into();
            // lookup component from sorted map
            let mut result: Vec<Component> = Vec::new();
            let ckey = Component {
                ctype,
                name: String::from(name),
                layer: String::default(),
            };

            for (c, _) in self.components.range(ckey..) {
                if c.name != name || c.ctype != ctype {
                    break;
                }
                result.push(c.clone());
            }
            result
        } else if let Some(ctype) = &ctype {
            let ctype: u16 = ctype.clone().into();
            // lookup component from sorted map
            let mut result: Vec<Component> = Vec::new();
            let ckey = Component {
                ctype,
                name: String::default(),
                layer: String::default(),
            };

            for (c, _) in self.components.range(ckey..) {
                if c.ctype != ctype {
                    break;
                }
                result.push(c.clone());
            }
            result
        } else {
            // filter all entries
            let filtered_components =
                self.components
                    .keys()
                    .cloned()
                    .filter(move |c: &Component| {
                        if let Some(ctype) = ctype.clone() {
                            let ctype: u16 = ctype.into();
                            if ctype != c.ctype {
                                return false;
                            }
                        }
                        if let Some(name) = name {
                            if name != c.name {
                                return false;
                            }
                        }
                        true
                    });
            filtered_components.collect()
        }
    }

    pub fn size_of_cached(&self, ops: &mut MallocSizeOfOps) -> usize {
        let mut lock = self.cached_size.lock().unwrap();
        let cached_size: &mut Option<usize> = &mut *lock;
        if let Some(cached) = cached_size {
            return *cached;
        }
        let calculated_size = self.size_of(ops);
        *cached_size = Some(calculated_size);
        calculated_size
    }

    fn reset_cached_size(&self) {
        let mut lock = self.cached_size.lock().unwrap();
        let cached_size: &mut Option<usize> = &mut *lock;
        *cached_size = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AnnoKey, Annotation, DefaultComponentType, Edge};

    #[test]
    fn create_writeable_gs() {
        let mut db = Graph::<DefaultComponentType>::new(false).unwrap();

        let anno_key = AnnoKey {
            ns: "test".to_owned(),
            name: "edge_anno".to_owned(),
        };
        let anno_val = "testValue".to_owned();

        let gs: &mut dyn WriteableGraphStorage = db
            .get_or_create_writable(&Component {
                ctype: DefaultComponentType::Edge.into(),
                layer: String::from("test"),
                name: String::from("dep"),
            })
            .unwrap();

        gs.add_edge(Edge {
            source: 0,
            target: 1,
        })
        .unwrap();

        gs.add_edge_annotation(
            Edge {
                source: 0,
                target: 1,
            },
            Annotation {
                key: anno_key,
                val: anno_val,
            },
        )
        .unwrap();
    }
}
