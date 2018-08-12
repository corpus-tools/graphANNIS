use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Debug)]
pub enum Factor {
    Statement(VecDeque<Statement>),
    Disjunction(Disjunction),
}

pub type Conjunction = VecDeque<Factor>;
pub type Disjunction = VecDeque<Conjunction>;

#[derive(Debug)]
pub struct InputPosition {
    start : usize,
    end: usize,
}

#[derive(Debug)]
pub enum Statement {
    TokenSearch{val : TextSearch, pos : Option<InputPosition>},
    AnnoSearch{name : QName, val : Option<TextSearch>, pos: Option<InputPosition>},
    BinaryOp {lhs : Operand, op: BinaryOpSpec, rhs : Operand, pos : Option<InputPosition>},
}

#[derive(Debug, Clone)]
pub enum Operand {
    NodeRef(NodeRef),
    Statement(Rc<Statement>)
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