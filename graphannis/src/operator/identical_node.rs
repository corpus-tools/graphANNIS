use super::*;

use {Component, Annotation};
use graphdb::GraphDB;
use std;

pub struct IdenticalNodeSpec;

impl OperatorSpec for IdenticalNodeSpec {
    fn necessary_components(&self) -> Vec<Component> {vec![]}

    fn create_operator<'a>(&self, _db: &'a GraphDB) -> Option<Box<Operator + 'a>> {
        Some(Box::new(IdenticalNode {}))
    }
   
}

pub struct IdenticalNode;

impl Operator for IdenticalNode {
    fn retrieve_matches<'a>(&'a self, lhs : &Match) -> Box<Iterator<Item = Match> + 'a> {
        return Box::new(std::iter::once(
            Match{node: lhs.node.clone(), anno: Annotation::default()}
            )
        );
    }

    fn filter_match(&self, lhs : &Match, rhs : &Match) -> bool {
        return lhs.node == rhs.node;
    }
}