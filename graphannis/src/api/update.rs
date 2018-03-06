#[derive(Serialize, Deserialize, Clone)]
pub enum UpdateEvent {
    AddNode {
        node_name: String,
        node_type: String,
    },
    DeleteNode {
        node_name: String,
    },
    AddNodeLabel {
        node_name: String,
        anno_ns: String,
        anno_name: String,
        anno_value: String,
    },
    DeleteNodeLabel {
        node_name: String,
        anno_ns: String,
        anno_name: String,
    },
    AddEdge {
        source_node: String,
        target_node: String,
        layer: String,
        component_type: String,
        component_name: String,
    },
    DeleteEdge {
        source_node: String,
        target_node: String,
        layer: String,
        component_type: String,
        component_name: String,
    },
    AddEdgeLabel {
        source_node: String,
        target_node: String,
        layer: String,
        component_type: String,
        component_name: String,
        anno_ns: String,
        anno_name: String,
        anno_value: String,
    },
    DeleteEdgeLabel {
        source_node: String,
        target_node: String,
        layer: String,
        component_type: String,
        component_name: String,
        anno_ns: String,
        anno_name: String,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GraphUpdate {
    diffs : Vec<(u64, UpdateEvent)>,
    last_consistent_change_id : u64,
}

impl GraphUpdate {
    pub fn new() -> GraphUpdate {
        GraphUpdate {
            diffs: vec![],
            last_consistent_change_id: 0,
        }
    }

    pub fn add_event(&mut self, event : UpdateEvent) {
        let change_id = self.last_consistent_change_id + (self.diffs.len() as u64)  + 1;
        self.diffs.push((change_id, event));
    }

    pub fn is_consistent(&self) -> bool {
        if self.diffs.is_empty() {
            return true;
        } else {
            return self.last_consistent_change_id == self.diffs[self.diffs.len()-1].0;
        }
    }

    pub fn into_consistent_changes_iter(self) -> Box<Iterator<Item=UpdateEvent>> {
        let last_consistent_change_id = self.last_consistent_change_id.clone();
        let it = self.diffs.into_iter().filter_map(move |d| {
            if d.0 <= last_consistent_change_id {
                Some(d.1)
            } else {
                None
            }
        });

        return Box::new(it);
    }
}