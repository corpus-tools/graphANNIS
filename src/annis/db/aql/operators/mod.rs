use std;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RangeSpec {
    Bound { min_dist: usize, max_dist: usize },
    Unbound,
}

impl RangeSpec {
    pub fn max_dist(&self) -> std::ops::Bound<usize> {
        match &self {
            RangeSpec::Bound { max_dist, .. } => std::ops::Bound::Included(*max_dist),
            RangeSpec::Unbound => std::ops::Bound::Unbounded,
        }
    }

    pub fn min_dist(&self) -> usize {
        match &self {
            RangeSpec::Bound { min_dist, .. } => *min_dist,
            RangeSpec::Unbound => 1,
        }
    }
}

impl fmt::Display for RangeSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            RangeSpec::Bound { min_dist, max_dist } => {
                if *min_dist == 1 && *max_dist == 1 {
                    write!(f, "")
                } else {
                    write!(f, "{},{}", min_dist, max_dist)
                }
            }
            RangeSpec::Unbound => write!(f, "*"),
        }
    }
}

pub mod edge_op;
pub mod identical_cov;
pub mod identical_node;
pub mod inclusion;
pub mod overlap;
pub mod precedence;
pub mod leftalignment;

pub use self::edge_op::{DominanceSpec, PartOfSubCorpusSpec, PointingSpec};
pub use self::identical_cov::IdenticalCoverageSpec;
pub use self::identical_node::IdenticalNodeSpec;
pub use self::inclusion::InclusionSpec;
pub use self::overlap::OverlapSpec;
pub use self::leftalignment::LeftAlignmentSpec;
pub use self::precedence::PrecedenceSpec;

