use graphstorage::EdgeContainer;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::collections::Bound::*;
use std::any::Any;
use std::clone::Clone;
use std;

use {NodeID, Edge, Annotation, AnnoKey, Match, NumValue};
use super::{GraphStorage, GraphStatistic};
use annostorage::AnnoStorage;
use graphdb::{GraphDB};
use dfs::{CycleSafeDFS, DFSStep};

#[derive(PartialOrd, PartialEq, Ord,Eq,Clone,Serialize, Deserialize,HeapSizeOf)]
pub struct PrePost<OrderT,LevelT> {
    pub pre : OrderT,
    pub post : OrderT,
    pub level : LevelT,
}

#[derive(Serialize, Deserialize,Clone,HeapSizeOf)]
pub struct PrePostOrderStorage<OrderT : NumValue, LevelT : NumValue> {
    
    node_to_order : HashMap<NodeID, Vec<PrePost<OrderT,LevelT>>>,
    order_to_node : BTreeMap<PrePost<OrderT,LevelT>,NodeID>,
    annos: AnnoStorage<Edge>,
    stats : Option<GraphStatistic>,
}

struct NodeStackEntry<OrderT, LevelT>
{
  pub id : NodeID,
  pub order : PrePost<OrderT,LevelT>,
}



impl<OrderT, LevelT>  PrePostOrderStorage<OrderT,LevelT> 
where OrderT : NumValue, 
    LevelT : NumValue {

    pub fn new() -> PrePostOrderStorage<OrderT, LevelT> {
        PrePostOrderStorage {
            node_to_order: HashMap::new(),
            order_to_node: BTreeMap::new(),
            annos: AnnoStorage::new(),
            stats: None,
        }
    }

    pub fn clear(&mut self) {
        self.node_to_order.clear();
        self.order_to_node.clear();
        self.annos.clear();
        self.stats = None;
    }


    fn enter_node(current_order : &mut OrderT, node_id : &NodeID, level : LevelT, node_stack : &mut NStack<OrderT,LevelT>) {
        let new_entry = NodeStackEntry {
            id: node_id.clone(),
            order : PrePost {
                pre: current_order.clone(),
                level: level,
                post : OrderT::zero(),
            },
        };
        current_order.add_assign(OrderT::one());
        node_stack.push_front(new_entry);
    }

    fn exit_node(&mut self, current_order : &mut OrderT, node_stack : &mut NStack<OrderT,LevelT>) {
         // find the correct pre/post entry and update the post-value
        if let Some(entry) = node_stack.front_mut() {
            entry.order.post = current_order.clone();
            current_order.add_assign(OrderT::one());

            self.node_to_order.entry(entry.id).or_insert(vec![]).push(entry.order.clone());
            self.order_to_node.insert(entry.order.clone(), entry.id);

        }
        node_stack.pop_front();
    }
}

type NStack<OrderT,LevelT> = std::collections::LinkedList<NodeStackEntry<OrderT,LevelT>>;

struct OrderIterEntry<OrderT,LevelT> {
    pub root : PrePost<OrderT,LevelT>,
    pub current: PrePost<OrderT,LevelT>,
    pub node: NodeID, 
}

impl<OrderT: 'static, LevelT : 'static> EdgeContainer for  PrePostOrderStorage<OrderT,LevelT> 
where OrderT : NumValue, 
    LevelT : NumValue {

    fn get_outgoing_edges<'a>(&'a self, source: &NodeID) -> Box<Iterator<Item = NodeID> + 'a> {
        return self.find_connected(source, 1, 1);
    }

    fn get_edge_annos(&self, edge : &Edge) -> Vec<Annotation> {
        return self.annos.get_all(edge);
    }

    fn get_anno_storage(&self) -> &AnnoStorage<Edge> {
        &self.annos
    }

    fn source_nodes<'a>(&'a self) -> Box<Iterator<Item = NodeID> + 'a> {
        let it = self.node_to_order.iter()
            .filter_map(move |(n, _order)| {
                // check if this is actual a source node (and not only a target node)
                if self.get_outgoing_edges(n).next().is_some() {
                    return Some(n.clone());
                } else {
                    return None;
                }
            });
        return Box::new(it);
    }

    fn get_statistics(&self) -> Option<&GraphStatistic> {self.stats.as_ref()}

}

impl<OrderT: 'static, LevelT : 'static> GraphStorage for  PrePostOrderStorage<OrderT,LevelT> 
where OrderT : NumValue, 
    LevelT : NumValue {


    
    fn find_connected<'a>(
        &'a self,
        source: &NodeID,
        min_distance: usize,
        max_distance: usize,
    ) -> Box<Iterator<Item = NodeID> + 'a> {
        
        if let Some(start_orders) = self.node_to_order.get(source) {
            let mut visited = HashSet::<NodeID>::new();
        
            let it = start_orders.into_iter()
                .flat_map(move |root_order : &PrePost<OrderT, LevelT>| {
                    let start_range : PrePost<OrderT,LevelT> = PrePost {
                        pre: root_order.pre.clone(),
                        post: OrderT::zero(),
                        level: LevelT::zero(),
                    };
                    let end_range : PrePost<OrderT,LevelT> = PrePost {
                        pre: root_order.post.clone(),
                        post: OrderT::max_value(),
                        level: LevelT::max_value(),
                    };
                    self.order_to_node
                        .range((Included(start_range),Included(end_range)))
                        .map(move |o| -> OrderIterEntry<OrderT,LevelT> { 
                            OrderIterEntry{
                                root: root_order.clone(), 
                                current: o.0.clone(), 
                                node: o.1.clone()}
                        }) 
                })
                .filter(move |o : &OrderIterEntry<OrderT,LevelT>| {
                    if let (Some(current_level), Some(root_level)) = (o.current.level.to_usize(), o.root.level.to_usize()) {
                        let diff_level = current_level - root_level;
                        return o.current.post <= o.root.post 
                            && min_distance <= diff_level && diff_level <= max_distance;
                    } else {
                        return false;
                    }
                })
                .map(|o : OrderIterEntry<OrderT,LevelT>| o.node)
                .filter(move |n| visited.insert(n.clone()));
            return Box::new(it);
        } else {
            return Box::new(std::iter::empty());
        }
    }

    fn distance(&self, source: &NodeID, target: &NodeID) -> Option<usize> {
        if source == target {
            return Some(0);
        }

        let mut min_level = usize::max_value();
        let mut was_found = false;

        if let (Some(order_source), Some(order_target)) = (self.node_to_order.get(source),self.node_to_order.get(target)) {
            for order_source in order_source.iter() {
                for order_target in order_target.iter() {
                    if order_source.pre <= order_target.pre && order_target.post <= order_source.post {
                        // check the level
                        if let (Some(source_level), Some(target_level)) = (order_source.level.to_usize(), order_target.level.to_usize()) {
                            if source_level <= target_level {
                                was_found = true;
                                min_level = std::cmp::min(target_level - source_level, min_level);
                            }
                        }
                    }
                }
            }            
        }

        if was_found {
            return Some(min_level);
        } else {
            return None;
        }
    }
    fn is_connected(&self, source: &NodeID, target: &NodeID, min_distance: usize, max_distance: usize) -> bool {
        
        if let (Some(order_source), Some(order_target)) = (self.node_to_order.get(source),self.node_to_order.get(target)) {
            for order_source in order_source.iter() {
                for order_target in order_target.iter() {
                    if order_source.pre <= order_target.pre && order_target.post <= order_source.post {
                        // check the level
                        if let (Some(source_level), Some(target_level)) = (order_source.level.to_usize(), order_target.level.to_usize()) {
                            if source_level <= target_level {
                                let diff_level = target_level-source_level;
                                return min_distance <= diff_level && diff_level <= max_distance;
                            }
                        }
                    }
                }
            }            
        }

        return false;
    }

    

    fn copy(&mut self, db : &GraphDB, orig : &EdgeContainer) {

        self.clear();

        // find all roots of the component
        let mut roots : HashSet<NodeID> = HashSet::new();
        let node_name_key : AnnoKey = db.get_node_name_key();
        let nodes : Box<Iterator<Item = Match>> = 
            db.node_annos.exact_anno_search(Some(node_name_key.ns), node_name_key.name, None);

        // first add all nodes that are a source of an edge as possible roots
        for m in nodes {
            let m : Match = m;
            let n = m.node;
            // insert all nodes to the root candidate list which are part of this component
            if orig.get_outgoing_edges(&n).next().is_some() {
                roots.insert(n);
            }
        }

        let nodes : Box<Iterator<Item = Match>> = 
            db.node_annos.exact_anno_search(Some(node_name_key.ns), node_name_key.name, None);
        for m in nodes {
            let m : Match = m;

            let source = m.node;

            let out_edges = orig.get_outgoing_edges(&source);
            for target in out_edges {
                // remove the nodes that have an incoming edge from the root list
                roots.remove(&target);

                // add the edge annotations for this edge
                let e = Edge {source, target};
                let edge_annos = orig.get_edge_annos(&e);
                for a in edge_annos.into_iter() {
                    self.annos.insert(e.clone(), a);
                }
            }
        }

        let mut current_order = OrderT::zero();
        // traverse the graph for each sub-component
        for start_node in roots.iter() {
            let mut last_distance : usize = 0;

            let mut node_stack : NStack<OrderT,LevelT> = NStack::new();

            PrePostOrderStorage::enter_node(&mut current_order, start_node, LevelT::zero(), &mut node_stack);

            let dfs = CycleSafeDFS::new(orig, start_node, 1, usize::max_value());
            for step in dfs {
                let step : DFSStep = step;
                if step.distance > last_distance {
                    // first visited, set pre-order
                    if let Some(dist) = LevelT::from_usize(step.distance) {
                        PrePostOrderStorage::enter_node(&mut current_order, &step.node, dist, &mut node_stack);
                    }
                } else {
                    // Neighbour node, the last subtree was iterated completly, thus the last node
                    // can be assigned a post-order.
                    // The parent node must be at the top of the node stack,
                    // thus exit every node which comes after the parent node.
                    // Distance starts with 0 but the stack size starts with 1.
                    while node_stack.len() > step.distance {
                        self.exit_node(&mut current_order, &mut node_stack);
                    }
                    // new node
                    if let Some(dist) = LevelT::from_usize(step.distance) {
                        PrePostOrderStorage::enter_node(&mut current_order, &step.node, dist, &mut node_stack);
                    }
                }
                last_distance = step.distance;
            } // end for each DFS step

            while !node_stack.is_empty() {
                self.exit_node(&mut current_order,&mut node_stack);
            }
        } // end for each root

        self.stats = orig.get_statistics().cloned();
        self.annos.calculate_statistics(&db.strings);

        self.node_to_order.shrink_to_fit();
    }

    fn as_any(&self) -> &Any {self}

    fn as_edgecontainer(&self) -> &EdgeContainer {self}
    

}
