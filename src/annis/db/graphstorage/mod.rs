use annis::db::AnnotationStorage;
use annis::db::Graph;
use annis::errors::*;
use annis::types::{AnnoKey, Annotation, Edge, NodeID};
use bincode;
use malloc_size_of::MallocSizeOf;
use serde::Deserialize;
use std;

/// Some general statistical numbers specific to a graph component
#[derive(Serialize, Deserialize, Clone, MallocSizeOf)]
pub struct GraphStatistic {
    /// True if the component contains any cycle.
    pub cyclic: bool,

    /// True if the component consists of a [rooted trees](https://en.wikipedia.org/wiki/Tree_(graph_theory)).
    pub rooted_tree: bool,

    /// Number of nodes in this graph storage (both source and target nodes).
    pub nodes: usize,

    /// Average fan out.format_args!
    pub avg_fan_out: f64,
    /// Max fan-out of 99% of the data.
    pub fan_out_99_percentile: usize,
    /// Maximal number of children of a node.
    pub max_fan_out: usize,
    /// Maximum length from a root node to a terminal node.
    pub max_depth: usize,

    /// Only valid for acyclic graphs: the average number of times a DFS will visit each node.
    pub dfs_visit_ratio: f64,
}

/// Basic trait for accessing edges of a graph for a specific [component](types/struct.Component.html).
pub trait EdgeContainer: Sync + Send + MallocSizeOf {
    /// Get all outgoing edges for a given `node`.
    fn get_outgoing_edges<'a>(&'a self, node: &NodeID) -> Box<Iterator<Item = NodeID> + 'a>;

    /// Get all incoming edges for a given `node`.
    fn get_ingoing_edges<'a>(&'a self, node: &NodeID) -> Box<Iterator<Item = NodeID> + 'a>;

    /// Get the annotation storage for the edges of this container.
    fn get_anno_storage(&self) -> &AnnotationStorage<Edge>;

    fn get_statistics(&self) -> Option<&GraphStatistic> {
        None
    }

    /// Provides an iterator over all nodes of this edge container that are the source an edge
    fn source_nodes<'a>(&'a self) -> Box<Iterator<Item = NodeID> + 'a>;
}

/// A graph storage is the representation of an edge component of a graph with specific structures.
/// These specific structures are exploited to efficiently implement reachability queries.
pub trait GraphStorage: EdgeContainer {

    /// Find all nodes reachable from a given start node inside the component.
    fn find_connected<'a>(
        &'a self,
        node: &NodeID,
        min_distance: usize,
        max_distance: usize,
    ) -> Box<Iterator<Item = NodeID> + 'a>;

    /// Find all nodes reachable from a given start node inside the component, when the directed edges are inversed.
    fn find_connected_inverse<'a>(
        &'a self,
        node: &NodeID,
        min_distance: usize,
        max_distance: usize,
    ) -> Box<Iterator<Item = NodeID> + 'a>;

    /// Compute the distance (shortest path length) of two nodes inside this component.
    fn distance(&self, source: &NodeID, target: &NodeID) -> Option<usize>;

    /// Check if two nodes are connected with any path in this component given a minimum (`min_distance`) and maximum (`max_distance`) path length.
    fn is_connected(
        &self,
        source: &NodeID,
        target: &NodeID,
        min_distance: usize,
        max_distance: usize,
    ) -> bool;

    /// Copy the content of another component.
    /// This removes the existing content of this graph storage.
    fn copy(&mut self, db: &Graph, orig: &EdgeContainer);

    fn as_edgecontainer(&self) -> &EdgeContainer;

    fn as_writeable(&mut self) -> Option<&mut WriteableGraphStorage> {
        None
    }

    // TODO: use an actual cost model for graph storage access
    fn inverse_has_same_cost(&self) -> bool {
        false
    }

    fn calculate_statistics(&mut self) {}

    fn serialization_id(&self) -> String;

    fn serialize_gs(&self, writer: &mut std::io::Write) -> Result<()>;

    fn deserialize_gs(input: &mut std::io::Read) -> Result<Self>
    where
        for<'de> Self: std::marker::Sized + Deserialize<'de>,
    {
        let result = bincode::deserialize_from(input)?;
        Ok(result)
    }
}

pub trait WriteableGraphStorage: GraphStorage {
    fn add_edge(&mut self, edge: Edge);
    fn add_edge_annotation(&mut self, edge: Edge, anno: Annotation);

    fn delete_edge(&mut self, edge: &Edge);
    fn delete_edge_annotation(&mut self, edge: &Edge, anno_key: &AnnoKey);
    fn delete_node(&mut self, node: &NodeID);
}

pub mod adjacencylist;
pub mod linear;
pub mod prepost;
pub mod registry;
