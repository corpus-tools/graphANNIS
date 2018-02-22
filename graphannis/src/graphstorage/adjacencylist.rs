use super::*;
use annostorage::AnnoStorage;
use Edge;
use dfs::CycleSafeDFS;

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::collections::Bound::*;

#[derive(Serialize, Deserialize, Clone)]
pub struct AdjacencyListStorage {
    edges: BTreeSet<Edge>,
    inverse_edges: BTreeSet<Edge>,
    annos: AnnoStorage<Edge>,
    stats : Option<GraphStatistic>,
}

impl AdjacencyListStorage {
    pub fn new() -> AdjacencyListStorage {
        AdjacencyListStorage {
            edges: BTreeSet::new(),
            inverse_edges: BTreeSet::new(),
            annos: AnnoStorage::new(),
            stats: None,
        }
    }

}

impl GraphStorage for AdjacencyListStorage {

     fn get_outgoing_edges<'a>(&'a self, source : &NodeID) -> Box<Iterator<Item=NodeID> + 'a> {
        let start_key = Edge{source: source.clone(), target: NodeID::min_value()};
        let end_key = Edge {source: source.clone(), target: NodeID::max_value()};

        let it = self.edges.range(start_key..end_key)
            .map(|e| {
                e.target
            });

        return Box::new(it);

    }

    fn get_edge_annos(&self, edge : &Edge) -> Vec<Annotation> {
        self.annos.get_all(edge)
    }

    fn find_connected<'a>(
        &'a self,
        source: &NodeID,
        min_distance: usize,
        max_distance: usize,
    ) -> Box<Iterator<Item = NodeID> + 'a> {
        let mut visited = HashSet::<NodeID>::new();
        let it = CycleSafeDFS::<'a>::new(self, source, min_distance, max_distance)
            .map(|x| {x.0})
            .filter(move |n| visited.insert(n.clone()));
        Box::new(it)
    }

    fn distance(&self, source: &NodeID, target: &NodeID) -> Option<usize> {
        let mut it = CycleSafeDFS::new(self, source, usize::min_value(), usize::max_value())
        .filter(|x| *target == x.0 )
        .map(|x| x.1);

        return it.next();

    }
    fn is_connected(&self, source: &NodeID, target: &NodeID, min_distance: usize, max_distance: usize) -> bool {
        let mut it = CycleSafeDFS::new(self, source, min_distance, max_distance)
        .filter(|x| *target == x.0 );
        
        return it.next().is_some();
    }

    fn copy(&mut self, _other : &GraphStorage) {
        unimplemented!();
    }

    fn as_writeable(&mut self) -> Option<&mut WriteableGraphStorage> {Some(self)}

    fn as_any(&self) -> &Any {self}

}

impl WriteableGraphStorage for AdjacencyListStorage {
    fn add_edge(&mut self, edge: Edge) {
        if edge.source != edge.target {
            self.inverse_edges.insert(edge.inverse());
            self.edges.insert(edge);
            // TODO: invalid graph statistics
        }
    }
    fn add_edge_annotation(&mut self, edge: Edge, anno: Annotation) {
        if self.edges.contains(&edge) {
            self.annos.insert(edge, anno);
        }
    }

    fn delete_edge(&mut self, edge: &Edge) {
        self.edges.remove(edge);
        self.inverse_edges.remove(&edge.inverse());

        let annos = self.annos.get_all(edge);
        for a in annos {
            self.annos.remove(edge, &a.key);
        }
    }
    fn delete_edge_annotation(&mut self, edge: &Edge, anno_key: &AnnoKey) {
        self.annos.remove(edge, anno_key);
    }
    fn delete_node(&mut self, node: &NodeID) {
        // find all both ingoing and outgoing edges
        let mut to_delete = std::collections::LinkedList::<Edge>::new();

        let range_start = Edge {
            source: node.clone(),
            target: NodeID::min_value(),
        };
        let range_end = Edge {
            source: node.clone(),
            target: NodeID::max_value(),
        };

        for e in self.edges
            .range((Included(range_start.clone()), Included(range_end.clone())))
        {
            to_delete.push_back(e.clone());
        }

        for e in self.inverse_edges
            .range((Included(range_start), Included(range_end)))
        {
            to_delete.push_back(e.clone());
        }

        for e in to_delete {
            self.delete_edge(&e);
        }
    }


    fn calculate_statistics(&mut self) {
        let mut stats = GraphStatistic {
            max_depth: 1,
            max_fan_out: 0,
            avg_fan_out: 0.0,
            fan_out_99_percentile: 0,
            cyclic: false,
            rooted_tree : false,
            nodes: 0,
            dfs_visit_ratio: 0.0,
        };

        let mut sum_fan_out = 0;
        let mut has_incoming_edge : BTreeSet<NodeID> = BTreeSet::new();

         // find all root nodes
        let mut roots : BTreeSet<NodeID> = BTreeSet::new();
        {

            let mut all_nodes : BTreeSet<NodeID> = BTreeSet::new();
            for e in self.edges.iter() {
                roots.insert(e.source);
                all_nodes.insert(e.source);
                all_nodes.insert(e.target);
                
                if stats.rooted_tree {
                    if has_incoming_edge.contains(&e.target) {
                        stats.rooted_tree = false;
                    } else {
                        has_incoming_edge.insert(e.target);
                    }
                }
            }
            stats.nodes = all_nodes.len();
        }

        let mut fan_outs : Vec<usize> = Vec::new();
        let mut last_source_id : Option<NodeID> = None;
        let mut current_fan_out = 0;
        if !self.edges.is_empty() {
            for e in self.edges.iter() {
                roots.remove(&e.target);

                if let Some(last) = last_source_id {
                    if last != e.source {
                        stats.max_fan_out = std::cmp::max(stats.max_fan_out, current_fan_out);
                        sum_fan_out += current_fan_out;
                        fan_outs.push(current_fan_out);

                        current_fan_out = 0;
                        last_source_id = Some(e.source);
                    }
                }
                current_fan_out += 1;
            }
            // add the statistics for the last node
            stats.max_fan_out = std::cmp::max(stats.max_fan_out, current_fan_out);
            sum_fan_out += current_fan_out;
            fan_outs.push(current_fan_out);
        }
        // order the fan-outs
        fan_outs.sort();

        // get the percentile value(s)
        // set some default values in case there are not enough elements in the component
        if ! fan_outs.is_empty() {
            stats.fan_out_99_percentile = fan_outs[fan_outs.len()-1];
        }
        // calculate the more accurate values
        if fan_outs.len() >= 100 {
            let idx : usize = fan_outs.len() / 100;
            if idx < fan_outs.len() {
                stats.fan_out_99_percentile = fan_outs[idx];
            }
        }

        let mut number_of_visits = 0;
        if roots.is_empty() && !self.edges.is_empty() {
            // if we have edges but no roots at all there must be a cycle
            stats.cyclic = true;
        } else {
            for root_node in roots.iter() {
                let mut dfs = CycleSafeDFS::new(self, &root_node, 0, usize::max_value());
                while let Some((_, depth)) = dfs.next() {
                    number_of_visits += 1;
                    stats.max_depth = std::cmp::max(stats.max_depth, depth);
                }
                if dfs.is_cyclic() {
                    stats.cyclic = true;
                }
            }
        }

        if stats.cyclic {
            stats.rooted_tree = false;
            // it's infinite
            stats.max_depth = 0;
            stats.dfs_visit_ratio = 0.0; 
        }
        else
        {
            if stats.nodes > 0 {
                stats.dfs_visit_ratio = (number_of_visits as f64) / (stats.nodes as f64);
            }
        }

        if sum_fan_out > 0 && stats.nodes > 0 {
            stats.avg_fan_out = (sum_fan_out as f64) / (stats.nodes as f64);
        }

        // TODO: also calculate the annotation statistics

        self.stats = Some(stats);
    }
}



#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn simple_dag_find_all() {
        /*
        +---+     +---+     +---+     +---+
        | 7 | <-- | 5 | <-- | 3 | <-- | 1 |
        +---+     +---+     +---+     +---+
                    |         |         |
                    |         |         |
                    v         |         v
                  +---+       |       +---+
                  | 6 |       |       | 2 |
                  +---+       |       +---+
                              |         |
                              |         |
                              |         v
                              |       +---+
                              +-----> | 4 |
                                      +---+
        */
        let mut gs = AdjacencyListStorage::new();

        gs.add_edge(Edge {
            source: 1,
            target: 2,
        });
        gs.add_edge(Edge {
            source: 2,
            target: 4,
        });
        gs.add_edge(Edge {
            source: 1,
            target: 3,
        });
        gs.add_edge(Edge {
            source: 3,
            target: 5,
        });
        gs.add_edge(Edge {
            source: 5,
            target: 7,
        });
        gs.add_edge(Edge {
            source: 5,
            target: 6,
        });
        gs.add_edge(Edge {
            source: 3,
            target: 4,
        });

        assert_eq!(vec![2, 3], gs.get_outgoing_edges(&1).collect::<Vec<NodeID>>());
        assert_eq!(vec![4,5], gs.get_outgoing_edges(&3).collect::<Vec<NodeID>>());
        assert_eq!(0, gs.get_outgoing_edges(&6).count());
        assert_eq!(vec![4], gs.get_outgoing_edges(&2).collect::<Vec<NodeID>>());

        let mut reachable : Vec<NodeID> = gs.find_connected(&1, 1, 100).collect();
        reachable.sort();
        assert_eq!(vec![2,3,4,5,6,7], reachable);

        let mut reachable : Vec<NodeID> = gs.find_connected(&3, 2, 100).collect();
        reachable.sort();
        assert_eq!(vec![6,7], reachable);

        let mut reachable : Vec<NodeID> = gs.find_connected(&1, 2, 4).collect();
        reachable.sort();
        assert_eq!(vec![4,5,6,7], reachable);

        let reachable : Vec<NodeID> = gs.find_connected(&7, 1, 100).collect();
        assert_eq!(true, reachable.is_empty());

        
    }

}
