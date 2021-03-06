use super::disjunction::Disjunction;
use super::Config;
use crate::annis::db::exec::filter::Filter;
use crate::annis::db::exec::indexjoin::IndexJoin;
use crate::annis::db::exec::nestedloop::NestedLoop;
use crate::annis::db::exec::nodesearch::{NodeSearch, NodeSearchSpec};
use crate::annis::db::exec::parallel;
use crate::annis::db::exec::{CostEstimate, Desc, ExecutionNode, NodeSearchDesc};
use crate::annis::db::{aql::model::AnnotationComponentType, AnnotationStorage};
use crate::annis::errors::*;
use crate::annis::operator::{
    BinaryOperator, BinaryOperatorSpec, UnaryOperator, UnaryOperatorSpec,
};
use crate::AnnotationGraph;
use crate::{
    annis::types::{LineColumnRange, QueryAttributeDescription},
    errors::Result,
};
use graphannis_core::{
    annostorage::MatchGroup,
    graph::storage::GraphStatistic,
    types::{Component, Edge},
};
use rand::distributions::Distribution;
use rand::distributions::Uniform;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug)]
struct BinaryOperatorSpecEntry<'a> {
    op: Box<dyn BinaryOperatorSpec + 'a>,
    idx_left: usize,
    idx_right: usize,
    global_reflexivity: bool,
}

#[derive(Debug)]
struct UnaryOperatorSpecEntry<'a> {
    op: Box<dyn UnaryOperatorSpec + 'a>,
    idx: usize,
}

pub struct BinaryOperatorEntry<'a> {
    pub op: Box<dyn BinaryOperator + 'a>,
    pub node_nr_left: usize,
    pub node_nr_right: usize,
    pub global_reflexivity: bool,
}

pub struct UnaryOperatorEntry {
    pub op: Box<dyn UnaryOperator>,
    pub node_nr: usize,
}

#[derive(Debug)]
pub struct Conjunction<'a> {
    nodes: Vec<(String, NodeSearchSpec)>,
    binary_operators: Vec<BinaryOperatorSpecEntry<'a>>,
    unary_operators: Vec<UnaryOperatorSpecEntry<'a>>,
    variables: HashMap<String, usize>,
    location_in_query: HashMap<String, LineColumnRange>,
    include_in_output: HashSet<String>,
    var_idx_offset: usize,
}

fn update_components_for_nodes(
    node2component: &mut BTreeMap<usize, usize>,
    from: usize,
    to: usize,
) {
    if from == to {
        // nothing todo
        return;
    }

    let mut node_ids_to_update: Vec<usize> = Vec::new();
    for (k, v) in node2component.iter() {
        if *v == from {
            node_ids_to_update.push(*k);
        }
    }

    // set the component id for each node of the other component
    for nid in &node_ids_to_update {
        node2component.insert(*nid, to);
    }
}

fn should_switch_operand_order(
    op_spec: &BinaryOperatorSpecEntry,
    node2cost: &BTreeMap<usize, CostEstimate>,
) -> bool {
    if let (Some(cost_lhs), Some(cost_rhs)) = (
        node2cost.get(&op_spec.idx_left),
        node2cost.get(&op_spec.idx_right),
    ) {
        let cost_lhs: &CostEstimate = cost_lhs;
        let cost_rhs: &CostEstimate = cost_rhs;

        if cost_rhs.output < cost_lhs.output {
            // switch operands
            return true;
        }
    }

    false
}

fn create_join<'b>(
    db: &'b AnnotationGraph,
    config: &Config,
    op_entry: BinaryOperatorEntry<'b>,
    exec_left: Box<dyn ExecutionNode<Item = MatchGroup> + 'b>,
    exec_right: Box<dyn ExecutionNode<Item = MatchGroup> + 'b>,
    idx_left: usize,
    idx_right: usize,
) -> Box<dyn ExecutionNode<Item = MatchGroup> + 'b> {
    if exec_right.as_nodesearch().is_some() {
        // use index join
        if config.use_parallel_joins {
            let join = parallel::indexjoin::IndexJoin::new(
                exec_left,
                idx_left,
                op_entry,
                exec_right.as_nodesearch().unwrap().get_node_search_desc(),
                db.get_node_annos(),
                exec_right.get_desc(),
            );
            return Box::new(join);
        } else {
            let join = IndexJoin::new(
                exec_left,
                idx_left,
                op_entry,
                exec_right.as_nodesearch().unwrap().get_node_search_desc(),
                db.get_node_annos(),
                exec_right.get_desc(),
            );
            return Box::new(join);
        }
    } else if exec_left.as_nodesearch().is_some() {
        // avoid a nested loop join by switching the operand and using and index join
        if let Some(inverse_op) = op_entry.op.get_inverse_operator(db) {
            if config.use_parallel_joins {
                let join = parallel::indexjoin::IndexJoin::new(
                    exec_right,
                    idx_right,
                    BinaryOperatorEntry {
                        node_nr_left: op_entry.node_nr_right,
                        node_nr_right: op_entry.node_nr_left,
                        op: inverse_op,
                        global_reflexivity: op_entry.global_reflexivity,
                    },
                    exec_left.as_nodesearch().unwrap().get_node_search_desc(),
                    db.get_node_annos(),
                    exec_left.get_desc(),
                );
                return Box::new(join);
            } else {
                let join = IndexJoin::new(
                    exec_right,
                    idx_right,
                    BinaryOperatorEntry {
                        node_nr_left: op_entry.node_nr_right,
                        node_nr_right: op_entry.node_nr_left,
                        op: inverse_op,
                        global_reflexivity: op_entry.global_reflexivity,
                    },
                    exec_left.as_nodesearch().unwrap().get_node_search_desc(),
                    db.get_node_annos(),
                    exec_left.get_desc(),
                );
                return Box::new(join);
            }
        }
    }

    // use nested loop as "fallback"
    if config.use_parallel_joins {
        let join = parallel::nestedloop::NestedLoop::new(
            op_entry, exec_left, exec_right, idx_left, idx_right,
        );
        Box::new(join)
    } else {
        let join = NestedLoop::new(op_entry, exec_left, exec_right, idx_left, idx_right);
        Box::new(join)
    }
}

impl<'a> Conjunction<'a> {
    pub fn new() -> Conjunction<'a> {
        Conjunction {
            nodes: vec![],
            binary_operators: vec![],
            unary_operators: vec![],
            variables: HashMap::default(),
            location_in_query: HashMap::default(),
            include_in_output: HashSet::default(),
            var_idx_offset: 0,
        }
    }

    pub fn with_offset(var_idx_offset: usize) -> Conjunction<'a> {
        Conjunction {
            nodes: vec![],
            binary_operators: vec![],
            unary_operators: vec![],
            variables: HashMap::default(),
            location_in_query: HashMap::default(),
            include_in_output: HashSet::default(),
            var_idx_offset,
        }
    }

    pub fn into_disjunction(self) -> Disjunction<'a> {
        Disjunction::new(vec![self])
    }

    pub fn get_node_descriptions(&self) -> Vec<QueryAttributeDescription> {
        let mut result = Vec::default();
        for (var, spec) in &self.nodes {
            let anno_name = match spec {
                NodeSearchSpec::ExactValue { name, .. } => Some(name.clone()),
                NodeSearchSpec::RegexValue { name, .. } => Some(name.clone()),
                _ => None,
            };
            let desc = QueryAttributeDescription {
                alternative: 0,
                query_fragment: format!("{}", spec),
                variable: var.clone(),
                anno_name,
            };
            result.push(desc);
        }
        result
    }

    pub fn add_node(&mut self, node: NodeSearchSpec, variable: Option<&str>) -> String {
        self.add_node_from_query(node, variable, None, true)
    }

    pub fn add_node_from_query(
        &mut self,
        node: NodeSearchSpec,
        variable: Option<&str>,
        location: Option<LineColumnRange>,
        included_in_output: bool,
    ) -> String {
        let idx = self.var_idx_offset + self.nodes.len();
        let variable = if let Some(variable) = variable {
            variable.to_string()
        } else {
            (idx + 1).to_string()
        };
        self.nodes.push((variable.clone(), node));
        self.variables.insert(variable.clone(), idx);
        if included_in_output {
            self.include_in_output.insert(variable.clone());
        }
        if let Some(location) = location {
            self.location_in_query.insert(variable.clone(), location);
        }
        variable
    }

    pub fn add_unary_operator_from_query(
        &mut self,
        op: Box<dyn UnaryOperatorSpec>,
        var: &str,
        location: Option<LineColumnRange>,
    ) -> Result<()> {
        if let Some(idx) = self.variables.get(var) {
            self.unary_operators
                .push(UnaryOperatorSpecEntry { op, idx: *idx });
            Ok(())
        } else {
            Err(GraphAnnisError::AQLSemanticError(AQLError {
                desc: format!("Operand '#{}' not found", var),
                location,
            }))
        }
    }

    pub fn add_operator(
        &mut self,
        op: Box<dyn BinaryOperatorSpec>,
        var_left: &str,
        var_right: &str,
        global_reflexivity: bool,
    ) -> Result<()> {
        self.add_operator_from_query(op, var_left, var_right, None, global_reflexivity)
    }

    pub fn add_operator_from_query(
        &mut self,
        op: Box<dyn BinaryOperatorSpec>,
        var_left: &str,
        var_right: &str,
        location: Option<LineColumnRange>,
        global_reflexivity: bool,
    ) -> Result<()> {
        //let original_order = self.operators.len();
        let idx_left = self.resolve_variable_pos(var_left, location.clone())?;
        let idx_right = self.resolve_variable_pos(var_right, location)?;

        self.binary_operators.push(BinaryOperatorSpecEntry {
            op,
            idx_left,
            idx_right,
            global_reflexivity,
        });
        Ok(())
    }

    pub fn num_of_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn resolve_variable_pos(
        &self,
        variable: &str,
        location: Option<LineColumnRange>,
    ) -> Result<usize> {
        if let Some(pos) = self.variables.get(variable) {
            return Ok(*pos);
        }
        Err(GraphAnnisError::AQLSemanticError(AQLError {
            desc: format!("Operand '#{}' not found", variable),
            location,
        }))
    }

    pub fn is_included_in_output(&self, variable: &str) -> bool {
        self.include_in_output.contains(variable)
    }

    pub fn get_variable_by_pos(&self, pos: usize) -> Option<String> {
        if pos < self.nodes.len() {
            return Some(self.nodes[pos].0.clone());
        }
        None
    }

    pub fn resolve_variable(
        &self,
        variable: &str,
        location: Option<LineColumnRange>,
    ) -> Result<NodeSearchSpec> {
        let idx = self.resolve_variable_pos(variable, location.clone())?;
        if let Some(pos) = idx.checked_sub(self.var_idx_offset) {
            if pos < self.nodes.len() {
                return Ok(self.nodes[pos].1.clone());
            }
        }

        Err(GraphAnnisError::AQLSemanticError(AQLError {
            desc: format!("Operand '#{}' not found", variable),
            location,
        }))
    }

    pub fn necessary_components(
        &self,
        db: &AnnotationGraph,
    ) -> HashSet<Component<AnnotationComponentType>> {
        let mut result = HashSet::default();

        for op_entry in &self.unary_operators {
            let c = op_entry.op.necessary_components(db);
            result.extend(c);
        }

        for op_entry in &self.binary_operators {
            let c = op_entry.op.necessary_components(db);
            result.extend(c);
        }
        for n in &self.nodes {
            result.extend(n.1.necessary_components(db));
        }

        result
    }

    fn optimize_join_order_heuristics(
        &self,
        db: &'a AnnotationGraph,
        config: &Config,
    ) -> Result<Vec<usize>> {
        // check if there is something to optimize
        if self.binary_operators.is_empty() {
            return Ok(vec![]);
        } else if self.binary_operators.len() == 1 {
            return Ok(vec![0]);
        }

        // use a constant seed to make the result deterministic
        let mut rng = SmallRng::from_seed(*b"Graphs are great");
        let dist = Uniform::from(0..self.binary_operators.len());

        let mut best_operator_order: Vec<_> = (0..self.binary_operators.len()).collect();

        // TODO: cache the base estimates
        let initial_plan =
            self.make_exec_plan_with_order(db, config, best_operator_order.clone())?;
        let mut best_cost: usize = initial_plan
            .get_desc()
            .ok_or(GraphAnnisError::PlanDescriptionMissing)?
            .cost
            .clone()
            .ok_or(GraphAnnisError::PlanCostMissing)?
            .intermediate_sum;
        trace!(
            "initial plan:\n{}",
            initial_plan
                .get_desc()
                .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                .debug_string("  ")
        );

        let num_new_generations = 4;
        let max_unsuccessful_tries = 5 * self.binary_operators.len();
        let mut unsucessful = 0;
        while unsucessful < max_unsuccessful_tries {
            let mut family_operators: Vec<Vec<usize>> = Vec::new();
            family_operators.reserve(num_new_generations + 1);

            family_operators.push(best_operator_order.clone());

            for i in 0..num_new_generations {
                // use the the previous generation as basis
                let mut tmp_operators = family_operators[i].clone();
                // randomly select two joins
                let mut a = 0;
                let mut b = 0;
                while a == b {
                    a = dist.sample(&mut rng);
                    b = dist.sample(&mut rng);
                }
                // switch the order of the selected joins
                tmp_operators.swap(a, b);
                family_operators.push(tmp_operators);
            }

            let mut found_better_plan = false;
            for op_order in family_operators.iter().skip(1) {
                let alt_plan = self.make_exec_plan_with_order(db, config, op_order.clone())?;
                let alt_cost = alt_plan
                    .get_desc()
                    .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                    .cost
                    .clone()
                    .ok_or(GraphAnnisError::PlanCostMissing)?
                    .intermediate_sum;
                trace!(
                    "alternatives plan: \n{}",
                    initial_plan
                        .get_desc()
                        .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                        .debug_string("  ")
                );

                if alt_cost < best_cost {
                    best_operator_order = op_order.clone();
                    found_better_plan = true;
                    trace!("Found better plan");
                    best_cost = alt_cost;
                    unsucessful = 0;
                }
            }

            if !found_better_plan {
                unsucessful += 1;
            }
        }

        Ok(best_operator_order)
    }

    fn optimize_node_search_by_operator(
        &'a self,
        node_search_desc: Arc<NodeSearchDesc>,
        desc: Option<&Desc>,
        op_spec_entries: Box<dyn Iterator<Item = &'a BinaryOperatorSpecEntry> + 'a>,
        db: &'a AnnotationGraph,
    ) -> Option<Box<dyn ExecutionNode<Item = MatchGroup> + 'a>> {
        let desc = desc?;
        // check if we can replace this node search with a generic "all nodes from either of these components" search
        let node_search_cost: &CostEstimate = desc.cost.as_ref()?;

        for e in op_spec_entries {
            let op_spec = &e.op;
            if e.idx_left == desc.component_nr {
                // get the necessary components and count the number of nodes in these components
                let components = op_spec.necessary_components(db);
                if !components.is_empty() {
                    let mut estimated_component_search = 0;

                    let mut estimation_valid = false;
                    for c in &components {
                        if let Some(gs) = db.get_graphstorage(c) {
                            // check if we can apply an even more restrictive edge annotation search
                            if let Some(edge_anno_spec) = op_spec.get_edge_anno_spec() {
                                let anno_storage: &dyn AnnotationStorage<Edge> =
                                    gs.get_anno_storage();
                                let edge_anno_est = edge_anno_spec.guess_max_count(anno_storage);
                                estimated_component_search += edge_anno_est;
                                estimation_valid = true;
                            } else if let Some(stats) = gs.get_statistics() {
                                let stats: &GraphStatistic = stats;
                                estimated_component_search += stats.nodes;
                                estimation_valid = true;
                            }
                        }
                    }

                    if estimation_valid && node_search_cost.output > estimated_component_search {
                        let poc_search = NodeSearch::new_partofcomponentsearch(
                            db,
                            node_search_desc,
                            Some(desc),
                            components,
                            op_spec.get_edge_anno_spec(),
                        );
                        if let Ok(poc_search) = poc_search {
                            // TODO: check if there is another operator with even better estimates
                            return Some(Box::new(poc_search));
                        } else {
                            return None;
                        }
                    }
                }
            }
        }

        None
    }

    fn make_exec_plan_with_order(
        &'a self,
        db: &'a AnnotationGraph,
        config: &Config,
        operator_order: Vec<usize>,
    ) -> Result<Box<dyn ExecutionNode<Item = MatchGroup> + 'a>> {
        let mut node2component: BTreeMap<usize, usize> = BTreeMap::new();

        // Remember node search errors, but do not bail out of this function before the component
        // semantics check has been performed.
        let mut node_search_errors: Vec<GraphAnnisError> = Vec::default();

        // 1. add all nodes

        // Create a map where the key is the component number
        // and move all nodes with their index as component number.
        let mut component2exec: BTreeMap<usize, Box<dyn ExecutionNode<Item = MatchGroup> + 'a>> =
            BTreeMap::new();
        let mut node2cost: BTreeMap<usize, CostEstimate> = BTreeMap::new();

        for node_nr in 0..self.nodes.len() {
            let n_spec = &self.nodes[node_nr].1;
            let n_var = &self.nodes[node_nr].0;

            let node_search = NodeSearch::from_spec(
                n_spec.clone(),
                node_nr,
                db,
                self.location_in_query.get(n_var).cloned(),
            );
            match node_search {
                Ok(mut node_search) => {
                    node2component.insert(node_nr, node_nr);

                    let (orig_query_frag, orig_impl_desc, cost) =
                        if let Some(d) = node_search.get_desc() {
                            if let Some(ref c) = d.cost {
                                node2cost.insert(node_nr, c.clone());
                            }

                            (
                                d.query_fragment.clone(),
                                d.impl_description.clone(),
                                d.cost.clone(),
                            )
                        } else {
                            (String::from(""), String::from(""), None)
                        };
                    // make sure the description is correct
                    let mut node_pos = BTreeMap::new();
                    node_pos.insert(node_nr, 0);
                    let new_desc = Desc {
                        component_nr: node_nr,
                        lhs: None,
                        rhs: None,
                        node_pos,
                        impl_description: orig_impl_desc,
                        query_fragment: orig_query_frag,
                        cost,
                    };
                    node_search.set_desc(Some(new_desc));

                    let node_by_component_search = self.optimize_node_search_by_operator(
                        node_search.get_node_search_desc(),
                        node_search.get_desc(),
                        Box::new(self.binary_operators.iter()),
                        db,
                    );

                    // move to map
                    if let Some(node_by_component_search) = node_by_component_search {
                        component2exec.insert(node_nr, node_by_component_search);
                    } else {
                        component2exec.insert(node_nr, Box::new(node_search));
                    }
                }
                Err(e) => node_search_errors.push(e),
            };
        }

        // 2. add unary operators as filter to the existing node search
        for op_spec_entry in self.unary_operators.iter() {
            let child_exec: Box<dyn ExecutionNode<Item = MatchGroup> + 'a> = component2exec
                .remove(&op_spec_entry.idx)
                .ok_or(GraphAnnisError::NoExecutionNode(op_spec_entry.idx))?;

            let op: Box<dyn UnaryOperator> =
                op_spec_entry.op.create_operator(db).ok_or_else(|| {
                    GraphAnnisError::ImpossibleSearch(format!(
                        "could not create operator {:?}",
                        op_spec_entry
                    ))
                })?;
            let op_entry = UnaryOperatorEntry {
                op,
                node_nr: op_spec_entry.idx + 1,
            };
            let filter_exec = Filter::new_unary(child_exec, 0, op_entry);

            component2exec.insert(op_spec_entry.idx, Box::new(filter_exec));
        }

        // 3. add the joins which produce the results in operand order
        for i in operator_order {
            let op_spec_entry: &BinaryOperatorSpecEntry<'a> = &self.binary_operators[i];

            let mut op: Box<dyn BinaryOperator + 'a> =
                op_spec_entry.op.create_operator(db).ok_or_else(|| {
                    GraphAnnisError::ImpossibleSearch(format!(
                        "could not create operator {:?}",
                        op_spec_entry
                    ))
                })?;

            let mut spec_idx_left = op_spec_entry.idx_left;
            let mut spec_idx_right = op_spec_entry.idx_right;

            let inverse_op = op.get_inverse_operator(db);
            if let Some(inverse_op) = inverse_op {
                if should_switch_operand_order(op_spec_entry, &node2cost) {
                    spec_idx_left = op_spec_entry.idx_right;
                    spec_idx_right = op_spec_entry.idx_left;

                    op = inverse_op;
                }
            }

            // substract the offset from the specificated numbers to get the internal node number for this conjunction
            spec_idx_left -= self.var_idx_offset;
            spec_idx_right -= self.var_idx_offset;

            let op_entry = BinaryOperatorEntry {
                op,
                node_nr_left: spec_idx_left + 1,
                node_nr_right: spec_idx_right + 1,
                global_reflexivity: op_spec_entry.global_reflexivity,
            };

            let component_left: usize = *(node2component
                .get(&spec_idx_left)
                .ok_or_else(|| GraphAnnisError::NoComponentForNode(spec_idx_left + 1))?);
            let component_right: usize = *(node2component
                .get(&spec_idx_right)
                .ok_or_else(|| GraphAnnisError::NoComponentForNode(spec_idx_right + 1))?);

            // get the original execution node
            let exec_left: Box<dyn ExecutionNode<Item = MatchGroup> + 'a> = component2exec
                .remove(&component_left)
                .ok_or(GraphAnnisError::NoExecutionNode(component_left))?;

            let idx_left: usize = *(exec_left
                .get_desc()
                .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                .node_pos
                .get(&spec_idx_left)
                .ok_or(GraphAnnisError::LHSOperandNotFound)?);

            let new_exec: Box<dyn ExecutionNode<Item = MatchGroup>> =
                if component_left == component_right {
                    // don't create new tuples, only filter the existing ones
                    // TODO: check if LHS or RHS is better suited as filter input iterator
                    let idx_right: usize = *(exec_left
                        .get_desc()
                        .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                        .node_pos
                        .get(&spec_idx_right)
                        .ok_or(GraphAnnisError::RHSOperandNotFound)?);

                    let filter = Filter::new_binary(exec_left, idx_left, idx_right, op_entry);
                    Box::new(filter)
                } else {
                    let exec_right = component2exec
                        .remove(&component_right)
                        .ok_or(GraphAnnisError::NoExecutionNode(component_right))?;
                    let idx_right: usize = *(exec_right
                        .get_desc()
                        .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                        .node_pos
                        .get(&spec_idx_right)
                        .ok_or(GraphAnnisError::RHSOperandNotFound)?);

                    create_join(
                        db, config, op_entry, exec_left, exec_right, idx_left, idx_right,
                    )
                };

            let new_component_nr = new_exec
                .get_desc()
                .ok_or(GraphAnnisError::PlanDescriptionMissing)?
                .component_nr;
            update_components_for_nodes(&mut node2component, component_left, new_component_nr);
            update_components_for_nodes(&mut node2component, component_right, new_component_nr);
            component2exec.insert(new_component_nr, new_exec);
        }

        // apply the the node error check
        if !node_search_errors.is_empty() {
            return Err(node_search_errors.remove(0));
        }

        // it must be checked before that all components are connected
        component2exec
            .into_iter()
            .map(|(_cid, exec)| exec)
            .next()
            .ok_or_else(|| {
                GraphAnnisError::ImpossibleSearch(String::from(
                    "could not find execution node for query component",
                ))
            })
    }

    fn check_components_connected(&self) -> Result<()> {
        let mut node2component: BTreeMap<usize, usize> = BTreeMap::new();
        node2component
            .extend((self.var_idx_offset..self.nodes.len() + self.var_idx_offset).map(|i| (i, i)));

        for op_entry in self.binary_operators.iter() {
            if op_entry.op.is_binding() {
                // merge both operands to the same component
                if let (Some(component_left), Some(component_right)) = (
                    node2component.get(&op_entry.idx_left),
                    node2component.get(&op_entry.idx_right),
                ) {
                    let component_left = *component_left;
                    let component_right = *component_right;
                    let new_component_nr = component_left;
                    update_components_for_nodes(
                        &mut node2component,
                        component_left,
                        new_component_nr,
                    );
                    update_components_for_nodes(
                        &mut node2component,
                        component_right,
                        new_component_nr,
                    );
                }
            }
        }

        // check if there is only one component left (all nodes are connected)
        let mut first_component_id: Option<usize> = None;
        for (node_nr, cid) in &node2component {
            if first_component_id.is_none() {
                first_component_id = Some(*cid);
            } else if let Some(first) = first_component_id {
                if first != *cid {
                    // add location and description which node is not connected
                    let n_var = &self.nodes[*node_nr].0;
                    let location = self.location_in_query.get(n_var);

                    return Err(GraphAnnisError::AQLSemanticError(AQLError {
                        desc: format!(
                            "Variable \"{}\" not bound (use linguistic operators)",
                            n_var
                        ),
                        location: location.cloned(),
                    }));
                }
            }
        }

        Ok(())
    }

    pub fn make_exec_node(
        &'a self,
        db: &'a AnnotationGraph,
        config: &Config,
    ) -> Result<Box<dyn ExecutionNode<Item = MatchGroup> + 'a>> {
        self.check_components_connected()?;

        let operator_order = self.optimize_join_order_heuristics(db, config)?;
        self.make_exec_plan_with_order(db, config, operator_order)
    }
}
