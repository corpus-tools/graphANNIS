use crate::annis::db::exec::{Desc, EmptyResultSet, ExecutionNode};
use crate::annis::db::query::disjunction::Disjunction;
use crate::annis::db::query::Config;
use crate::annis::db::{Graph, Match};
use crate::annis::errors::*;
use crate::annis::types::{AnnoKeyID, NodeID};
use std;
use std::collections::HashSet;
use std::fmt::Formatter;

pub struct ExecutionPlan<'a> {
    plans: Vec<Box<ExecutionNode<Item = Vec<Match>> + 'a>>,
    current_plan: usize,
    descriptions: Vec<Option<Desc>>,
    proxy_mode: bool,
    unique_result_set: HashSet<Vec<(NodeID, AnnoKeyID)>>,
}

impl<'a> ExecutionPlan<'a> {
    pub fn from_disjunction(
        query: &'a Disjunction<'a>,
        db: &'a Graph,
        config: &Config,
    ) -> Result<ExecutionPlan<'a>> {
        let mut plans: Vec<Box<ExecutionNode<Item = Vec<Match>> + 'a>> = Vec::new();
        let mut descriptions: Vec<Option<Desc>> = Vec::new();
        for alt in &query.alternatives {
            let p = alt.make_exec_node(db, &config);
            if let Ok(p) = p {
                descriptions.push(p.get_desc().cloned());
                plans.push(p);
            } else if let Err(e) = p {
                if let ErrorKind::AQLSemanticError(_, _) = e.kind() {
                    return Err(e);
                }
            }
        }

        if plans.is_empty() {
            // add a dummy execution step that yields no results
            let no_results_exec = EmptyResultSet {};
            plans.push(Box::new(no_results_exec));
            descriptions.push(None);
        }
        Ok(ExecutionPlan {
            current_plan: 0,
            descriptions,
            proxy_mode: plans.len() == 1,
            plans,
            unique_result_set: HashSet::new(),
        })
    }

    fn reorder_match(&self, tmp: Vec<Match>) -> Vec<Match> {
        if tmp.len() <= 1 {
            // nothing to reorder
            return tmp;
        }
        if let Some(ref desc) = self.descriptions[self.current_plan] {
            let desc: &Desc = desc;
            // re-order the matched nodes by the original node position of the query
            let mut result: Vec<Match> = Vec::with_capacity(tmp.len());
            for i in 0..tmp.len() {
                if let Some(mapped_pos) = desc.node_pos.get(&i) {
                    result.push(tmp[*mapped_pos].clone());
                } else {
                    result.push(tmp[i].clone());
                }
            }
            result
        } else {
            tmp
        }
    }

    pub fn estimated_output_size(&self) -> usize {
        let mut estimation = 0;
        for desc in &self.descriptions {
            if let Some(desc) = desc {
                if let Some(ref cost) = desc.cost {
                    estimation += cost.output;
                }
            }
        }
        estimation
    }

    pub fn is_sorted_by_text(&self) -> bool {
        if self.plans.len() > 1 {
            false
        } else if self.plans.is_empty() {
            true
        } else {
            self.plans[0].is_sorted_by_text()
        }
    }
}

impl<'a> std::fmt::Display for ExecutionPlan<'a> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (i, d) in self.descriptions.iter().enumerate() {
            if i > 0 {
                writeln!(f, "---[OR]---")?;
            }
            if let Some(ref d) = d {
                write!(f, "{}", d.debug_string(""))?;
            } else {
                write!(f, "<no description>")?;
            }
        }
        Ok(())
    }
}

impl<'a> Iterator for ExecutionPlan<'a> {
    type Item = Vec<Match>;

    fn next(&mut self) -> Option<Vec<Match>> {
        if self.proxy_mode {
            // just act as an proxy, but make sure the order is the same as requested in the query
            if let Some(n) = self.plans[0].next() {
                Some(self.reorder_match(n))
            } else {
                None
            }
        } else {
            while self.current_plan < self.plans.len() {
                if let Some(n) = self.plans[self.current_plan].next() {
                    let n = self.reorder_match(n);

                    // check if we already outputted this result
                    let key: Vec<(NodeID, AnnoKeyID)> =
                        n.iter().map(|m: &Match| (m.node, m.anno_key)).collect();
                    if self.unique_result_set.insert(key) {
                        // new result found, break out of while-loop and return the result
                        return Some(n);
                    }
                } else {
                    // proceed to next plan
                    self.current_plan += 1;
                }
            }
            None
        }
    }
}
