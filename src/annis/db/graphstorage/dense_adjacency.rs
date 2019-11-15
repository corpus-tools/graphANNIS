use crate::annis::db::annostorage::inmemory::AnnoStorageImpl;
use crate::annis::db::graphstorage::{EdgeContainer, GraphStatistic, GraphStorage};
use crate::annis::db::{AnnotationStorage, Graph};
use crate::annis::dfs::CycleSafeDFS;
use crate::annis::errors::*;
use crate::annis::types::{Edge, NodeID};
use num::ToPrimitive;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use std::ops::Bound;

#[derive(Serialize, Deserialize, Clone, MallocSizeOf)]
pub struct DenseAdjacencyListStorage {
    edges: Vec<Option<NodeID>>,
    inverse_edges: FxHashMap<NodeID, Vec<NodeID>>,
    annos: AnnoStorageImpl<Edge>,
    stats: Option<GraphStatistic>,
}

impl Default for DenseAdjacencyListStorage {
    fn default() -> Self {
        DenseAdjacencyListStorage::new()
    }
}

impl DenseAdjacencyListStorage {
    pub fn new() -> DenseAdjacencyListStorage {
        DenseAdjacencyListStorage {
            edges: Vec::default(),
            inverse_edges: FxHashMap::default(),
            annos: AnnoStorageImpl::new(),
            stats: None,
        }
    }
}

impl EdgeContainer for DenseAdjacencyListStorage {
    /// Get all outgoing edges for a given `node`.
    fn get_outgoing_edges<'a>(&'a self, node: NodeID) -> Box<dyn Iterator<Item = NodeID> + 'a> {
        if let Some(node) = node.to_usize() {
            if node < self.edges.len() {
                if let Some(outgoing) = self.edges[node] {
                    return Box::new(std::iter::once(outgoing));
                }
            }
        }
        Box::new(std::iter::empty())
    }

    fn get_ingoing_edges<'a>(&'a self, node: NodeID) -> Box<dyn Iterator<Item = NodeID> + 'a> {
        if let Some(ingoing) = self.inverse_edges.get(&node) {
            return match ingoing.len() {
                0 => Box::new(std::iter::empty()),
                1 => Box::new(std::iter::once(ingoing[0])),
                _ => Box::new(ingoing.iter().cloned()),
            };
        }
        Box::new(std::iter::empty())
    }

    fn get_statistics(&self) -> Option<&GraphStatistic> {
        self.stats.as_ref()
    }

    /// Provides an iterator over all nodes of this edge container that are the source an edge
    fn source_nodes<'a>(&'a self) -> Box<dyn Iterator<Item = NodeID> + 'a> {
        let it = self
            .edges
            .iter()
            .enumerate()
            .filter(|(_, outgoing)| outgoing.is_none())
            .filter_map(|(key, _)| key.to_u64());
        Box::new(it)
    }
}

impl GraphStorage for DenseAdjacencyListStorage {
    fn find_connected<'a>(
        &'a self,
        node: NodeID,
        min_distance: usize,
        max_distance: Bound<usize>,
    ) -> Box<dyn Iterator<Item = NodeID> + 'a> {
        let mut visited = FxHashSet::<NodeID>::default();
        let max_distance = match max_distance {
            Bound::Unbounded => usize::max_value(),
            Bound::Included(max_distance) => max_distance,
            Bound::Excluded(max_distance) => max_distance + 1,
        };
        let it = CycleSafeDFS::<'a>::new(self, node, min_distance, max_distance)
            .map(|x| x.node)
            .filter(move |n| visited.insert(n.clone()));
        Box::new(it)
    }

    fn find_connected_inverse<'a>(
        &'a self,
        node: NodeID,
        min_distance: usize,
        max_distance: Bound<usize>,
    ) -> Box<dyn Iterator<Item = NodeID> + 'a> {
        let mut visited = FxHashSet::<NodeID>::default();
        let max_distance = match max_distance {
            Bound::Unbounded => usize::max_value(),
            Bound::Included(max_distance) => max_distance,
            Bound::Excluded(max_distance) => max_distance + 1,
        };

        let it = CycleSafeDFS::<'a>::new_inverse(self, node, min_distance, max_distance)
            .map(|x| x.node)
            .filter(move |n| visited.insert(n.clone()));
        Box::new(it)
    }

    fn distance(&self, source: NodeID, target: NodeID) -> Option<usize> {
        let mut it = CycleSafeDFS::new(self, source, usize::min_value(), usize::max_value())
            .filter(|x| target == x.node)
            .map(|x| x.distance);

        it.next()
    }
    fn is_connected(
        &self,
        source: NodeID,
        target: NodeID,
        min_distance: usize,
        max_distance: std::ops::Bound<usize>,
    ) -> bool {
        let max_distance = match max_distance {
            Bound::Unbounded => usize::max_value(),
            Bound::Included(max_distance) => max_distance,
            Bound::Excluded(max_distance) => max_distance + 1,
        };
        let mut it = CycleSafeDFS::new(self, source, min_distance, max_distance)
            .filter(|x| target == x.node);

        it.next().is_some()
    }

    fn get_anno_storage(&self) -> &dyn AnnotationStorage<Edge> {
        &self.annos
    }

    fn copy(&mut self, db: &Graph, orig: &dyn GraphStorage) {
        self.annos.clear();
        self.edges.clear();
        self.inverse_edges.clear();

        if let Some(largest_idx) = db
            .node_annos
            .get_largest_item()
            .and_then(|idx| idx.to_usize())
        {
            debug!("Resizing dense adjacency list to size {}", largest_idx + 1);
            self.edges.resize(largest_idx + 1, None);

            for source in orig.source_nodes() {
                if let Some(idx) = source.to_usize() {
                    if let Some(target) = orig.get_outgoing_edges(source).next() {
                        // insert edge
                        self.edges[idx] = Some(target);

                        // insert inverse edge
                        let e = Edge { source, target };
                        let inverse_entry = self
                            .inverse_edges
                            .entry(e.target)
                            .or_insert_with(Vec::default);
                        // no need to insert it: edge already exists
                        if let Err(insertion_idx) = inverse_entry.binary_search(&e.source) {
                            inverse_entry.insert(insertion_idx, e.source);
                        }
                        // insert annotation
                        for a in orig.get_anno_storage().get_annotations_for_item(&e) {
                            self.annos.insert(e.clone(), a);
                        }
                    }
                }
            }
            self.stats = orig.get_statistics().cloned();
            self.annos.calculate_statistics();
        }
    }

    fn as_edgecontainer(&self) -> &dyn EdgeContainer {
        self
    }

    fn inverse_has_same_cost(&self) -> bool {
        true
    }

    /// Return an identifier for this graph storage which is used to distinguish the different graph storages when (de-) serialized.
    fn serialization_id(&self) -> String {
        "DenseAdjacencyListV1".to_owned()
    }

    /// Serialize this graph storage.
    fn serialize_gs(&self, writer: &mut dyn std::io::Write) -> Result<()> {
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    /// De-serialize this graph storage.
    fn deserialize_gs(input: &mut dyn std::io::Read) -> Result<Self>
    where
        for<'de> Self: std::marker::Sized + Deserialize<'de>,
    {
        let mut result: DenseAdjacencyListStorage = bincode::deserialize_from(input)?;
        result.annos.after_deserialization();
        Ok(result)
    }
}
