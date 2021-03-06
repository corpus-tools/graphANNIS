use super::MatchFilterFunc;
use super::{Desc, ExecutionNode, NodeSearchDesc};
use crate::annis::db::exec::tokensearch;
use crate::annis::db::exec::tokensearch::AnyTokenSearch;
use crate::annis::db::{aql::model::AnnotationComponentType, AnnotationStorage};
use crate::annis::errors::*;
use crate::annis::operator::EdgeAnnoSearchSpec;
use crate::annis::types::LineColumnRange;
use crate::AnnotationGraph;
use crate::{
    annis::{db::aql::model::TOKEN_KEY, util},
    graph::Match,
};
use graphannis_core::{
    annostorage::{MatchGroup, ValueSearch},
    graph::{storage::GraphStorage, NODE_TYPE_KEY},
    types::{Component, Edge, NodeID},
};
use itertools::Itertools;
use smallvec::smallvec;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

/// An [ExecutionNode](#impl-ExecutionNode) which wraps base node (annotation) searches.
pub struct NodeSearch<'a> {
    /// The actual search implementation
    it: Box<dyn Iterator<Item = MatchGroup> + 'a>,

    desc: Option<Desc>,
    node_search_desc: Arc<NodeSearchDesc>,
    is_sorted: bool,
}
#[derive(Clone, Debug, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub enum NodeSearchSpec {
    ExactValue {
        ns: Option<String>,
        name: String,
        val: Option<String>,
        is_meta: bool,
    },
    NotExactValue {
        ns: Option<String>,
        name: String,
        val: String,
        is_meta: bool,
    },
    RegexValue {
        ns: Option<String>,
        name: String,
        val: String,
        is_meta: bool,
    },
    NotRegexValue {
        ns: Option<String>,
        name: String,
        val: String,
        is_meta: bool,
    },
    ExactTokenValue {
        val: String,
        leafs_only: bool,
    },
    NotExactTokenValue {
        val: String,
    },
    RegexTokenValue {
        val: String,
        leafs_only: bool,
    },
    NotRegexTokenValue {
        val: String,
    },
    AnyToken,
    AnyNode,
}

impl NodeSearchSpec {
    pub fn new_exact(
        ns: Option<&str>,
        name: &str,
        val: Option<&str>,
        is_meta: bool,
    ) -> NodeSearchSpec {
        NodeSearchSpec::ExactValue {
            ns: ns.map(String::from),
            name: String::from(name),
            val: val.map(String::from),
            is_meta,
        }
    }

    pub fn necessary_components(
        &self,
        db: &AnnotationGraph,
    ) -> HashSet<Component<AnnotationComponentType>> {
        if let NodeSearchSpec::AnyToken = self {
            return tokensearch::AnyTokenSearch::necessary_components(db);
        }
        HashSet::default()
    }
}

impl fmt::Display for NodeSearchSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NodeSearchSpec::ExactValue {
                ref ns,
                ref name,
                ref val,
                ..
            } => {
                if ns.is_some() && val.is_some() {
                    write!(
                        f,
                        "{}:{}=\"{}\"",
                        ns.as_ref().unwrap(),
                        name,
                        val.as_ref().unwrap()
                    )
                } else if ns.is_some() {
                    write!(f, "{}:{}", ns.as_ref().unwrap(), name)
                } else if val.is_some() {
                    write!(f, "{}=\"{}\"", name, val.as_ref().unwrap())
                } else {
                    write!(f, "{}", name)
                }
            }
            NodeSearchSpec::NotExactValue {
                ref ns,
                ref name,
                ref val,
                ..
            } => {
                if let Some(ref ns) = ns {
                    write!(f, "{}:{}!=\"{}\"", ns, name, &val)
                } else {
                    write!(f, "{}!=\"{}\"", name, &val)
                }
            }
            NodeSearchSpec::RegexValue {
                ref ns,
                ref name,
                ref val,
                ..
            } => {
                if ns.is_some() {
                    write!(f, "{}:{}=/{}/", ns.as_ref().unwrap(), name, &val)
                } else {
                    write!(f, "{}=/{}/", name, &val)
                }
            }
            NodeSearchSpec::NotRegexValue {
                ref ns,
                ref name,
                ref val,
                ..
            } => {
                if let Some(ref ns) = ns {
                    write!(f, "{}:{}!=/{}/", ns, name, &val)
                } else {
                    write!(f, "{}!=/{}/", name, &val)
                }
            }
            NodeSearchSpec::ExactTokenValue {
                ref val,
                ref leafs_only,
            } => {
                if *leafs_only {
                    write!(f, "tok=\"{}\"", val)
                } else {
                    write!(f, "\"{}\"", val)
                }
            }
            NodeSearchSpec::NotExactTokenValue { ref val } => write!(f, "tok!=\"{}\"", val),
            NodeSearchSpec::RegexTokenValue {
                ref val,
                ref leafs_only,
            } => {
                if *leafs_only {
                    write!(f, "tok=/{}/", val)
                } else {
                    write!(f, "/{}/", val)
                }
            }
            NodeSearchSpec::NotRegexTokenValue { ref val } => write!(f, "tok!=/{}/", val),
            NodeSearchSpec::AnyToken => write!(f, "tok"),
            NodeSearchSpec::AnyNode => write!(f, "node"),
        }
    }
}

impl<'a> NodeSearch<'a> {
    pub fn from_spec(
        spec: NodeSearchSpec,
        node_nr: usize,
        db: &'a AnnotationGraph,
        location_in_query: Option<LineColumnRange>,
    ) -> Result<NodeSearch<'a>> {
        let query_fragment = format!("{}", spec);

        match spec {
            NodeSearchSpec::ExactValue {
                ns,
                name,
                val,
                is_meta,
            } => NodeSearch::new_annosearch_exact(
                db,
                (ns, name),
                val.into(),
                is_meta,
                &query_fragment,
                node_nr,
            ),
            NodeSearchSpec::NotExactValue {
                ns,
                name,
                val,
                is_meta,
            } => NodeSearch::new_annosearch_exact(
                db,
                (ns, name),
                ValueSearch::NotSome(val),
                is_meta,
                &query_fragment,
                node_nr,
            ),
            NodeSearchSpec::RegexValue {
                ns,
                name,
                val,
                is_meta,
            } => {
                // check if the regex can be replaced with an exact value search
                let is_regex = util::contains_regex_metacharacters(&val);
                if is_regex {
                    NodeSearch::new_annosearch_regex(
                        db,
                        (ns, name),
                        &val,
                        false,
                        is_meta,
                        super::NodeDescArg {
                            query_fragment,
                            node_nr,
                        },
                        location_in_query,
                    )
                } else {
                    NodeSearch::new_annosearch_exact(
                        db,
                        (ns, name),
                        ValueSearch::Some(val),
                        is_meta,
                        &query_fragment,
                        node_nr,
                    )
                }
            }
            NodeSearchSpec::NotRegexValue {
                ns,
                name,
                val,
                is_meta,
            } => {
                // check if the regex can be replaced with an exact value search
                let is_regex = util::contains_regex_metacharacters(&val);
                if is_regex {
                    NodeSearch::new_annosearch_regex(
                        db,
                        (ns, name),
                        &val,
                        true,
                        is_meta,
                        super::NodeDescArg {
                            query_fragment,
                            node_nr,
                        },
                        location_in_query,
                    )
                } else {
                    NodeSearch::new_annosearch_exact(
                        db,
                        (ns, name),
                        ValueSearch::NotSome(val),
                        is_meta,
                        &query_fragment,
                        node_nr,
                    )
                }
            }
            NodeSearchSpec::ExactTokenValue { val, leafs_only } => NodeSearch::new_tokensearch(
                db,
                ValueSearch::Some(val),
                leafs_only,
                false,
                &query_fragment,
                node_nr,
                location_in_query,
            ),
            NodeSearchSpec::NotExactTokenValue { val } => NodeSearch::new_tokensearch(
                db,
                ValueSearch::NotSome(val),
                true,
                false,
                &query_fragment,
                node_nr,
                location_in_query,
            ),
            NodeSearchSpec::RegexTokenValue { val, leafs_only } => NodeSearch::new_tokensearch(
                db,
                ValueSearch::Some(val),
                leafs_only,
                true,
                &query_fragment,
                node_nr,
                location_in_query,
            ),
            NodeSearchSpec::NotRegexTokenValue { val } => NodeSearch::new_tokensearch(
                db,
                ValueSearch::NotSome(val),
                true,
                true,
                &query_fragment,
                node_nr,
                location_in_query,
            ),
            NodeSearchSpec::AnyToken => {
                NodeSearch::new_anytoken_search(db, &query_fragment, node_nr)
            }
            NodeSearchSpec::AnyNode => {
                let it = db
                    .get_node_annos()
                    .exact_anno_search(
                        Some(&NODE_TYPE_KEY.ns),
                        &NODE_TYPE_KEY.name,
                        Some("node").into(),
                    )
                    .map(move |n| smallvec![n]);

                let filter_func: Box<
                    dyn Fn(&Match, &dyn AnnotationStorage<NodeID>) -> bool + Send + Sync,
                > = Box::new(move |m, node_annos| {
                    if let Some(val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                        val == "node"
                    } else {
                        false
                    }
                });

                let est_output = db.get_node_annos().guess_max_count(
                    Some(&NODE_TYPE_KEY.ns),
                    &NODE_TYPE_KEY.name,
                    "node",
                    "node",
                );
                let est_output = std::cmp::max(1, est_output);

                Ok(NodeSearch {
                    it: Box::new(it),
                    desc: Some(Desc::empty_with_fragment(
                        super::NodeDescArg {
                            query_fragment,
                            node_nr,
                        },
                        Some(est_output),
                    )),
                    node_search_desc: Arc::new(NodeSearchDesc {
                        qname: (
                            Some(NODE_TYPE_KEY.ns.clone().into()),
                            Some(NODE_TYPE_KEY.name.clone().into()),
                        ),
                        cond: vec![filter_func],
                        const_output: Some(NODE_TYPE_KEY.clone()),
                    }),
                    is_sorted: false,
                })
            }
        }
    }

    fn new_annosearch_exact(
        db: &'a AnnotationGraph,
        qname: (Option<String>, String),
        val: ValueSearch<String>,
        is_meta: bool,
        query_fragment: &str,
        node_nr: usize,
    ) -> Result<NodeSearch<'a>> {
        let base_it = db.get_node_annos().exact_anno_search(
            qname.0.as_deref(),
            &qname.1,
            val.as_ref().map(String::as_str),
        );

        let const_output = if is_meta {
            Some(NODE_TYPE_KEY.clone())
        } else {
            None
        };

        let base_it: Box<dyn Iterator<Item = Match>> =
            if let Some(const_output) = const_output.clone() {
                let is_unique = db.get_node_annos().get_qnames(&qname.1).len() <= 1;
                // Replace the result annotation with a constant value.
                // If a node matches two different annotations (because there is no namespace), this can result in duplicates which needs to be filtered out.
                if is_unique {
                    Box::new(base_it.map(move |m| Match {
                        node: m.node,
                        anno_key: const_output.clone(),
                    }))
                } else {
                    Box::new(
                        base_it
                            .map(move |m| Match {
                                node: m.node,
                                anno_key: const_output.clone(),
                            })
                            .unique(),
                    )
                }
            } else {
                base_it
            };

        let est_output = match val {
            ValueSearch::Some(ref val) => {
                db.get_node_annos()
                    .guess_max_count(qname.0.as_deref(), &qname.1, &val, &val)
            }
            ValueSearch::NotSome(ref val) => {
                let total = db
                    .get_node_annos()
                    .number_of_annotations_by_name(qname.0.as_deref(), &qname.1);
                total
                    - db.get_node_annos()
                        .guess_max_count(qname.0.as_deref(), &qname.1, &val, &val)
            }
            ValueSearch::Any => db
                .get_node_annos()
                .number_of_annotations_by_name(qname.0.as_deref(), &qname.1),
        };

        // always assume at least one output item otherwise very small selectivity can fool the planner
        let est_output = std::cmp::max(1, est_output);

        let it = base_it.map(|n| smallvec![n]);

        let mut filters: Vec<MatchFilterFunc> = Vec::new();

        match val {
            ValueSearch::Any => {}
            ValueSearch::Some(val) => {
                filters.push(Box::new(move |m, node_annos| {
                    if let Some(anno_val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                        anno_val == val.as_str()
                    } else {
                        false
                    }
                }));
            }
            ValueSearch::NotSome(val) => {
                filters.push(Box::new(move |m, node_annos| {
                    if let Some(anno_val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                        anno_val != val.as_str()
                    } else {
                        false
                    }
                }));
            }
        }
        Ok(NodeSearch {
            it: Box::new(it),
            desc: Some(Desc::empty_with_fragment(
                super::NodeDescArg {
                    query_fragment: query_fragment.to_owned(),
                    node_nr,
                },
                Some(est_output),
            )),
            node_search_desc: Arc::new(NodeSearchDesc {
                qname: (qname.0, Some(qname.1)),
                cond: filters,
                const_output,
            }),
            is_sorted: false,
        })
    }

    fn new_annosearch_regex(
        db: &'a AnnotationGraph,
        qname: (Option<String>, String),
        pattern: &str,
        negated: bool,
        is_meta: bool,
        node_desc_arg: super::NodeDescArg,
        location_in_query: Option<LineColumnRange>,
    ) -> Result<NodeSearch<'a>> {
        // match_regex works only with values
        let base_it =
            db.get_node_annos()
                .regex_anno_search(qname.0.as_deref(), &qname.1, pattern, negated);

        let const_output = if is_meta {
            Some(NODE_TYPE_KEY.clone())
        } else {
            None
        };

        let base_it: Box<dyn Iterator<Item = Match>> =
            if let Some(const_output) = const_output.clone() {
                let is_unique = db.get_node_annos().get_qnames(&qname.1).len() <= 1;
                // Replace the result annotation with a constant value.
                // If a node matches two different annotations (because there is no namespace), this can result in duplicates which needs to be filtered out.
                if is_unique {
                    Box::new(base_it.map(move |m| Match {
                        node: m.node,
                        anno_key: const_output.clone(),
                    }))
                } else {
                    Box::new(
                        base_it
                            .map(move |m| Match {
                                node: m.node,
                                anno_key: const_output.clone(),
                            })
                            .unique(),
                    )
                }
            } else {
                base_it
            };

        let est_output = if negated {
            let total = db
                .get_node_annos()
                .number_of_annotations_by_name(qname.0.as_deref(), &qname.1);
            total
                - db.get_node_annos()
                    .guess_max_count_regex(qname.0.as_deref(), &qname.1, pattern)
        } else {
            db.get_node_annos()
                .guess_max_count_regex(qname.0.as_deref(), &qname.1, pattern)
        };

        // always assume at least one output item otherwise very small selectivity can fool the planner
        let est_output = std::cmp::max(1, est_output);

        let it = base_it.map(|n| smallvec![n]);

        let mut filters: Vec<MatchFilterFunc> = Vec::new();

        let full_match_pattern = graphannis_core::util::regex_full_match(&pattern);
        let re = regex::Regex::new(&full_match_pattern);
        match re {
            Ok(re) => {
                if negated {
                    filters.push(Box::new(move |m, node_annos| {
                        if let Some(val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                            !re.is_match(&val)
                        } else {
                            false
                        }
                    }));
                } else {
                    filters.push(Box::new(move |m, node_annos| {
                        if let Some(val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                            re.is_match(&val)
                        } else {
                            false
                        }
                    }));
                }
            }
            Err(e) => {
                return Err(GraphAnnisError::AQLSemanticError(AQLError {
                    desc: format!("/{}/ -> {}", pattern, e),
                    location: location_in_query,
                }));
            }
        }

        Ok(NodeSearch {
            it: Box::new(it),
            desc: Some(Desc::empty_with_fragment(node_desc_arg, Some(est_output))),
            node_search_desc: Arc::new(NodeSearchDesc {
                qname: (qname.0, Some(qname.1)),
                cond: filters,
                const_output,
            }),
            is_sorted: false,
        })
    }

    fn new_tokensearch(
        db: &'a AnnotationGraph,
        val: ValueSearch<String>,
        leafs_only: bool,
        match_regex: bool,
        query_fragment: &str,
        node_nr: usize,
        location_in_query: Option<LineColumnRange>,
    ) -> Result<NodeSearch<'a>> {
        let it_base: Box<dyn Iterator<Item = Match>> = match val {
            ValueSearch::Any => {
                let it = db.get_node_annos().exact_anno_search(
                    Some(&TOKEN_KEY.ns),
                    &TOKEN_KEY.name,
                    None.into(),
                );
                Box::new(it)
            }
            ValueSearch::Some(ref val) => {
                let it = if match_regex {
                    db.get_node_annos().regex_anno_search(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        val,
                        false,
                    )
                } else {
                    db.get_node_annos().exact_anno_search(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        ValueSearch::Some(&val),
                    )
                };
                Box::new(it)
            }
            ValueSearch::NotSome(ref val) => {
                let it = if match_regex {
                    db.get_node_annos().regex_anno_search(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        val,
                        true,
                    )
                } else {
                    db.get_node_annos().exact_anno_search(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        ValueSearch::NotSome(val),
                    )
                };
                Box::new(it)
            }
        };

        let it_base = if leafs_only {
            let cov_gs: Vec<Arc<dyn GraphStorage>> = db
                .get_all_components(Some(AnnotationComponentType::Coverage), None)
                .into_iter()
                .filter_map(|c| db.get_graphstorage(&c))
                .filter(|gs| {
                    if let Some(stats) = gs.get_statistics() {
                        stats.nodes > 0
                    } else {
                        true
                    }
                })
                .collect();

            let it = it_base.filter(move |n| {
                for cov in cov_gs.iter() {
                    if cov.get_outgoing_edges(n.node).next().is_some() {
                        return false;
                    }
                }
                true
            });
            Box::new(it)
        } else {
            it_base
        };
        // map to vector
        let it = it_base.map(move |n| {
            smallvec![Match {
                node: n.node,
                anno_key: NODE_TYPE_KEY.clone(),
            }]
        });
        // create filter functions
        let mut filters: Vec<MatchFilterFunc> = Vec::new();

        match val {
            ValueSearch::Some(ref val) => {
                if match_regex {
                    let full_match_pattern = graphannis_core::util::regex_full_match(val);
                    let re = regex::Regex::new(&full_match_pattern);
                    match re {
                        Ok(re) => filters.push(Box::new(move |m, node_annos| {
                            if let Some(val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                                re.is_match(&val)
                            } else {
                                false
                            }
                        })),
                        Err(e) => {
                            return Err(GraphAnnisError::AQLSemanticError(AQLError {
                                desc: format!("/{}/ -> {}", val, e),
                                location: location_in_query,
                            }));
                        }
                    };
                } else {
                    let val = val.clone();
                    filters.push(Box::new(move |m, node_annos| {
                        if let Some(anno_val) = node_annos.get_value_for_item(&m.node, &m.anno_key)
                        {
                            anno_val == val.as_str()
                        } else {
                            false
                        }
                    }));
                };
            }
            ValueSearch::NotSome(ref val) => {
                if match_regex {
                    let full_match_pattern = graphannis_core::util::regex_full_match(val);
                    let re = regex::Regex::new(&full_match_pattern);
                    match re {
                        Ok(re) => filters.push(Box::new(move |m, node_annos| {
                            if let Some(val) = node_annos.get_value_for_item(&m.node, &m.anno_key) {
                                !re.is_match(&val)
                            } else {
                                false
                            }
                        })),
                        Err(e) => {
                            return Err(GraphAnnisError::AQLSemanticError(AQLError {
                                desc: format!("/{}/ -> {}", val, e),
                                location: location_in_query,
                            }));
                        }
                    };
                } else {
                    let val = val.clone();
                    filters.push(Box::new(move |m, node_annos| {
                        if let Some(anno_val) = node_annos.get_value_for_item(&m.node, &m.anno_key)
                        {
                            anno_val != val.as_str()
                        } else {
                            false
                        }
                    }));
                };
            }
            ValueSearch::Any => {}
        };

        if leafs_only {
            let cov_gs: Vec<Arc<dyn GraphStorage>> = db
                .get_all_components(Some(AnnotationComponentType::Coverage), None)
                .into_iter()
                .filter_map(|c| db.get_graphstorage(&c))
                .filter(|gs| {
                    if let Some(stats) = gs.get_statistics() {
                        stats.nodes > 0
                    } else {
                        true
                    }
                })
                .collect();

            let filter_func: Box<
                dyn Fn(&Match, &dyn AnnotationStorage<NodeID>) -> bool + Send + Sync,
            > = Box::new(move |m, _| {
                for cov in cov_gs.iter() {
                    if cov.get_outgoing_edges(m.node).next().is_some() {
                        return false;
                    }
                }
                true
            });
            filters.push(filter_func);
        };

        // TODO: is_leaf should be part of the estimation
        let est_output = match val {
            ValueSearch::Some(ref val) => {
                if match_regex {
                    db.get_node_annos().guess_max_count_regex(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        val,
                    )
                } else {
                    db.get_node_annos().guess_max_count(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        val,
                        val,
                    )
                }
            }
            ValueSearch::NotSome(val) => {
                let total_count = db
                    .get_node_annos()
                    .number_of_annotations_by_name(Some(&TOKEN_KEY.ns), &TOKEN_KEY.name);
                let positive_count = if match_regex {
                    db.get_node_annos().guess_max_count_regex(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        &val,
                    )
                } else {
                    db.get_node_annos().guess_max_count(
                        Some(&TOKEN_KEY.ns),
                        &TOKEN_KEY.name,
                        &val,
                        &val,
                    )
                };
                total_count - positive_count
            }
            ValueSearch::Any => db
                .get_node_annos()
                .number_of_annotations_by_name(Some(&TOKEN_KEY.ns), &TOKEN_KEY.name),
        };
        // always assume at least one output item otherwise very small selectivity can fool the planner
        let est_output = std::cmp::max(1, est_output);

        Ok(NodeSearch {
            it: Box::new(it),
            desc: Some(Desc::empty_with_fragment(
                super::NodeDescArg {
                    query_fragment: query_fragment.to_owned(),
                    node_nr,
                },
                Some(est_output),
            )),
            node_search_desc: Arc::new(NodeSearchDesc {
                qname: (
                    Some(TOKEN_KEY.ns.clone().into()),
                    Some(TOKEN_KEY.name.clone().into()),
                ),
                cond: filters,
                const_output: Some(NODE_TYPE_KEY.clone()),
            }),
            is_sorted: false,
        })
    }

    fn new_anytoken_search(
        db: &'a AnnotationGraph,
        query_fragment: &str,
        node_nr: usize,
    ) -> Result<NodeSearch<'a>> {
        let it: Box<dyn Iterator<Item = MatchGroup>> = Box::from(AnyTokenSearch::new(db)?);
        // create filter functions
        let mut filters: Vec<MatchFilterFunc> = Vec::new();

        let cov_gs: Vec<Arc<dyn GraphStorage>> = db
            .get_all_components(Some(AnnotationComponentType::Coverage), None)
            .into_iter()
            .filter_map(|c| db.get_graphstorage(&c))
            .filter(|gs| {
                if let Some(stats) = gs.get_statistics() {
                    stats.nodes > 0
                } else {
                    true
                }
            })
            .collect();

        let filter_func: MatchFilterFunc = Box::new(move |m, _| {
            for cov in cov_gs.iter() {
                if cov.get_outgoing_edges(m.node).next().is_some() {
                    return false;
                }
            }
            true
        });
        filters.push(filter_func);

        let est_output = db
            .get_node_annos()
            .number_of_annotations_by_name(Some(&TOKEN_KEY.ns), &TOKEN_KEY.name);
        // always assume at least one output item otherwise very small selectivity can fool the planner
        let est_output = std::cmp::max(1, est_output);

        Ok(NodeSearch {
            it: Box::new(it),
            desc: Some(Desc::empty_with_fragment(
                super::NodeDescArg {
                    query_fragment: query_fragment.to_owned(),
                    node_nr,
                },
                Some(est_output),
            )),
            node_search_desc: Arc::new(NodeSearchDesc {
                qname: (
                    Some(TOKEN_KEY.ns.clone().into()),
                    Some(TOKEN_KEY.name.clone().into()),
                ),
                cond: filters,
                const_output: Some(NODE_TYPE_KEY.clone()),
            }),
            is_sorted: true,
        })
    }

    pub fn new_partofcomponentsearch(
        db: &'a AnnotationGraph,
        node_search_desc: Arc<NodeSearchDesc>,
        desc: Option<&Desc>,
        components: HashSet<Component<AnnotationComponentType>>,
        edge_anno_spec: Option<EdgeAnnoSearchSpec>,
    ) -> Result<NodeSearch<'a>> {
        let node_search_desc_1 = node_search_desc.clone();
        let node_search_desc_2 = node_search_desc.clone();

        let it = components
            .into_iter()
            .flat_map(
                move |c: Component<AnnotationComponentType>| -> Box<dyn Iterator<Item = NodeID>> {
                    if let Some(gs) = db.get_graphstorage_as_ref(&c) {
                        if let Some(EdgeAnnoSearchSpec::ExactValue {
                            ref ns,
                            ref name,
                            ref val,
                        }) = edge_anno_spec
                        {
                            // for each component get the source nodes with this edge annotation
                            let anno_storage: &dyn AnnotationStorage<Edge> = gs.get_anno_storage();

                            let it = anno_storage
                                .exact_anno_search(
                                    ns.as_ref().map(String::as_str),
                                    name,
                                    val.as_ref().map(String::as_str).into(),
                                )
                                .map(|m: Match| m.node);
                            Box::new(it)
                        } else {
                            // for each component get the all its source nodes
                            gs.source_nodes()
                        }
                    } else {
                        Box::new(std::iter::empty())
                    }
                },
            )
            .flat_map(move |node: NodeID| {
                // fetch annotation candidates for the node based on the original description
                let node_search_desc = node_search_desc_1.clone();
                db.get_node_annos()
                    .get_all_keys_for_item(
                        &node,
                        node_search_desc.qname.0.as_deref(),
                        node_search_desc.qname.1.as_deref(),
                    )
                    .into_iter()
                    .map(move |anno_key| Match { node, anno_key })
            })
            .filter_map(move |m: Match| -> Option<MatchGroup> {
                // only include the nodes that fullfill all original node search predicates
                for cond in &node_search_desc_2.cond {
                    if !cond(&m, db.get_node_annos()) {
                        return None;
                    }
                }
                Some(smallvec![m])
            });
        let mut new_desc = desc.cloned();
        if let Some(ref mut new_desc) = new_desc {
            new_desc.impl_description = String::from("part-of-component-search");
        }
        Ok(NodeSearch {
            it: Box::new(it),
            desc: new_desc,
            node_search_desc,
            is_sorted: false,
        })
    }

    pub fn set_desc(&mut self, desc: Option<Desc>) {
        self.desc = desc;
    }

    pub fn get_node_search_desc(&self) -> Arc<NodeSearchDesc> {
        self.node_search_desc.clone()
    }
}

impl<'a> ExecutionNode for NodeSearch<'a> {
    fn as_iter(&mut self) -> &mut dyn Iterator<Item = MatchGroup> {
        self
    }

    fn get_desc(&self) -> Option<&Desc> {
        self.desc.as_ref()
    }

    fn as_nodesearch(&self) -> Option<&NodeSearch> {
        Some(self)
    }

    fn is_sorted_by_text(&self) -> bool {
        self.is_sorted
    }
}

impl<'a> Iterator for NodeSearch<'a> {
    type Item = MatchGroup;

    fn next(&mut self) -> Option<MatchGroup> {
        self.it.next()
    }
}
