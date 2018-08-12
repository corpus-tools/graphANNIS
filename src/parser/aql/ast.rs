use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Debug)]
pub enum Expr {
    Conjunction(Conjunction),
    Disjunction(Disjunction),
}

pub type Conjunction = VecDeque<Term>;
pub type Disjunction = VecDeque<Conjunction>;

#[derive(Debug)]
pub enum Term {
    TokenSearch(TextSearch),
    AnnoSearch(QName, Option<TextSearch>),
    BinaryOp(Operand, BinaryOpSpec, Operand),
    And(Rc<Term>, Rc<Term>),
    Or(Rc<Term>, Rc<Term>),
    Empty,
}

#[derive(Debug, Clone)]
pub enum Operand {
    NodeRef(NodeRef),
    Term(Rc<Term>)
}


#[derive(Debug)]
pub struct TextSearch(pub String, pub StringMatchType);   

#[derive(Debug)]
pub struct QName (pub Option<String>, pub String);


#[derive(Debug)]
pub enum StringMatchType {
    Exact,
    Regex,
}

#[derive(Debug)]
pub enum BinaryOpSpec {
    Dominance,
    Pointing,
    Precedence,
    Overlap,
    IdenticalCoverage,
}

#[derive(Debug,Clone)]
pub enum NodeRef {
    ID(u32),
    Name(String),
}