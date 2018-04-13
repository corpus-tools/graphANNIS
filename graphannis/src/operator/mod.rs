use operator::edge_op::EdgeAnnoSearchSpec;
use {Match, Component};
use graphdb::GraphDB;
use std;

pub enum EstimationType {
    SELECTIVITY(f64), MAX, MIN
}

pub trait Operator : std::fmt::Display {
    fn retrieve_matches<'a>(&'a self, lhs : &Match) -> Box<Iterator<Item = Match> + 'a>;

    fn filter_match(&self, lhs : &Match, rhs : &Match) -> bool;

    fn is_reflexive(&self) -> bool {true}
    fn is_commutative(&self) -> bool {false}

    fn estimation_type(&self) -> EstimationType {EstimationType::SELECTIVITY(0.1)}

    fn edge_anno_selectivity(&self) -> Option<f64> {
        None
    }
}

pub trait OperatorSpec : std::fmt::Debug {
    fn necessary_components(&self) -> Vec<Component>;

    fn create_operator(&self, db: &GraphDB) -> Option<Box<Operator>>;

    fn get_edge_anno_spec(&self) -> Option<EdgeAnnoSearchSpec> {None}
    
}

pub mod precedence;
pub mod edge_op;
pub mod identical_cov;
pub mod inclusion;
pub mod overlap;
pub mod identical_node;

pub use self::precedence::PrecedenceSpec;
pub use self::edge_op::{DominanceSpec, PointingSpec, PartOfSubCorpusSpec};
pub use self::inclusion::InclusionSpec;
pub use self::overlap::OverlapSpec;
pub use self::identical_cov::IdenticalCoverageSpec;
pub use self::identical_node::IdenticalNodeSpec;