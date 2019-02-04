use crate::annis::db::graphstorage::GraphStorage;
use crate::annis::db::token_helper;
use crate::annis::db::token_helper::TokenHelper;
use crate::annis::db::{Graph, Match};
use crate::annis::operator::EstimationType;
use crate::annis::operator::{BinaryOperator, BinaryOperatorSpec};
use crate::annis::types::{AnnoKeyID, Component, ComponentType};

use std;
use std::sync::Arc;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub struct IdenticalCoverageSpec;

#[derive(Clone)]
pub struct IdenticalCoverage {
    gs_left: Arc<GraphStorage>,
    gs_order: Arc<GraphStorage>,
    tok_helper: TokenHelper,
}

lazy_static! {
    static ref COMPONENT_LEFT: Component = {
        Component {
            ctype: ComponentType::LeftToken,
            layer: String::from("annis"),
            name: String::from(""),
        }
    };
    static ref COMPONENT_ORDER: Component = {
        Component {
            ctype: ComponentType::Ordering,
            layer: String::from("annis"),
            name: String::from(""),
        }
    };
}

impl BinaryOperatorSpec for IdenticalCoverageSpec {
    fn necessary_components(&self, db: &Graph) -> HashSet<Component> {
        let mut v = HashSet::new();
        v.insert(COMPONENT_LEFT.clone());
        v.insert(COMPONENT_ORDER.clone());
        v.extend(token_helper::necessary_components(db));
        v
    }

    fn create_operator(&self, db: &Graph) -> Option<Box<BinaryOperator>> {
        let optional_op = IdenticalCoverage::new(db);
        if let Some(op) = optional_op {
            return Some(Box::new(op));
        } else {
            return None;
        }
    }
}

impl IdenticalCoverage {
    pub fn new(db: &Graph) -> Option<IdenticalCoverage> {
        let gs_left = db.get_graphstorage(&COMPONENT_LEFT)?;
        let gs_order = db.get_graphstorage(&COMPONENT_ORDER)?;
        let tok_helper = TokenHelper::new(db)?;

        Some(IdenticalCoverage {
            gs_left,
            gs_order,
            tok_helper,
        })
    }
}

impl std::fmt::Display for IdenticalCoverage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "_=_")
    }
}

impl BinaryOperator for IdenticalCoverage {
    fn retrieve_matches(&self, lhs: &Match) -> Box<Iterator<Item = Match>> {
        let n_left = self.tok_helper.left_token_for(lhs.node);
        let n_right = self.tok_helper.right_token_for(lhs.node);

        let mut result: Vec<Match> = Vec::new();

        if n_left.is_some() && n_right.is_some() {
            let n_left = n_left.unwrap();
            let n_right = n_right.unwrap();

            if n_left == n_right {
                // covered range is exactly one token, add token itself
                result.push(Match {
                    node: n_left,
                    anno_key: AnnoKeyID::default(),
                });
            }

            // find left-aligned non-token
            let v = self.gs_left.get_ingoing_edges(n_left);
            for c in v {
                // check if also right-aligned
                if let Some(c_right) = self.tok_helper.right_token_for(c) {
                    if n_right == c_right {
                        result.push(Match {
                            node: c,
                            anno_key: AnnoKeyID::default(),
                        });
                    }
                }
            }
        }

        Box::new(result.into_iter())
    }

    fn filter_match(&self, lhs: &Match, rhs: &Match) -> bool {
        let start_lhs = self.tok_helper.left_token_for(lhs.node);
        let end_lhs = self.tok_helper.right_token_for(lhs.node);

        let start_rhs = self.tok_helper.left_token_for(rhs.node);
        let end_rhs = self.tok_helper.right_token_for(rhs.node);

        if start_lhs.is_none() || end_lhs.is_none() || start_rhs.is_none() || end_rhs.is_none() {
            return false;
        }

        start_lhs.unwrap() == start_rhs.unwrap() && end_lhs.unwrap() == end_rhs.unwrap()
    }

    fn is_reflexive(&self) -> bool {
        false
    }

    fn get_inverse_operator(&self) -> Option<Box<BinaryOperator>> {
        Some(Box::new(self.clone()))
    }

    fn estimation_type(&self) -> EstimationType {
        if let Some(order_stats) = self.gs_order.get_statistics() {
            let num_of_token = order_stats.nodes as f64;

            // Assume two nodes have same identical coverage if they have the same
            // left covered token and the same length (right covered token is not independent
            // of the left one, this is why we should use length).
            // The probability for the same length is taken is assumed to be 1.0, histograms
            // of the distribution would help here.

            EstimationType::SELECTIVITY(1.0 / num_of_token)
        } else {
            EstimationType::SELECTIVITY(0.1)
        }
    }
}
