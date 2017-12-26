use {Annotation, Match};
use operator::Operator;
use plan::{ExecutionNode,Desc};
use std;
use std::iter::Peekable;

/// A join that takes any iterator as left-hand-side (LHS) and an annotation condition as right-hand-side (RHS).
/// It then retrieves all matches as defined by the operator for each LHS element and checks
/// if the annotation condition is true.
pub struct IndexJoin<'a> {
    lhs: Peekable<Box<ExecutionNode<Item = Vec<Match>>+'a>>,
    rhs_candidate: std::vec::IntoIter<Match>,
    op: Box<Operator + 'a>,
    lhs_idx: usize,
    anno_cond: Box<Fn(Annotation) -> bool + 'a>,
    desc: Desc,
}

impl<'a> IndexJoin<'a> {

    /// Create a new `IndexJoin`
    /// # Arguments
    /// 
    /// * `lhs` - An iterator for a left-hand-side
    /// * `lhs_idx` - The index of the element in the LHS that should be used as a source
    /// * `op` - The operator that connects the LHS and RHS
    /// * `anno_cond` - A filter function to determine if a RHS candidate is included
    pub fn new(
        lhs: Box<ExecutionNode<Item = Vec<Match>> + 'a>,
        lhs_idx: usize,
        op: Box<Operator + 'a>,
        anno_cond: Box<Fn(Annotation) -> bool + 'a>,
        rhs_desc: Option<&Desc>,
    ) -> IndexJoin<'a> {
        let lhs_desc = lhs.get_desc().cloned();
        // TODO, we 
        let mut lhs_peek = lhs.peekable();
        let initial_candidates: Vec<Match> = if let Some(m_lhs) = lhs_peek.peek() {
            op.retrieve_matches(&m_lhs[lhs_idx.clone()]).collect()
        } else {
            vec![]
        };
        return IndexJoin {
            desc: Desc::join(lhs_desc.as_ref(), rhs_desc),
            lhs: lhs_peek,
            lhs_idx,
            op,
            anno_cond,
            rhs_candidate: initial_candidates.into_iter(),
        };
    }
}

impl<'a> ExecutionNode for IndexJoin<'a> {
    fn as_iter(&mut self) -> &mut Iterator<Item = Vec<Match>> {
        self
    }

    fn get_desc(&self) -> Option<&Desc> {
        Some(&self.desc)
    }
}


impl<'a> Iterator for IndexJoin<'a> {
    type Item = Vec<Match>;

    fn next(&mut self) -> Option<Vec<Match>> {
        loop {
            if let Some(m_lhs) = self.lhs.peek() {
                while let Some(m_rhs) = self.rhs_candidate.next() {
                    // filter by annotation
                    if (self.anno_cond)(m_rhs.anno.clone()) {
                        let mut result = m_lhs.clone();
                        result.push(m_rhs.clone());
                        return Some(result);
                    }
                }
                // inner was completed once, get new candidates
                let candidates: Vec<Match> =
                    self.op.retrieve_matches(&m_lhs[self.lhs_idx]).collect();
                self.rhs_candidate = candidates.into_iter();
            }

            // consume next outer
            if self.lhs.next().is_none() {
                return None;
            }
        }
    }
}
