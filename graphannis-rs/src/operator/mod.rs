use Match;

pub trait Operator {
    fn retrieve_matches(&self, lhs : &Match) -> Box<Iterator<Item = Match>>;

    fn filter_match(&self, lhs : &Match, rhs : &Match) -> bool;
}

pub mod precedence;