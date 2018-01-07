use super::{Desc, ExecutionNode, NodeSearchDesc};
use graphdb::{GraphDB, ANNIS_NS};
use {Annotation, Match, StringID, Component, ComponentType};

use std::rc::Rc;
use std::fmt;

/// An [ExecutionNode](#impl-ExecutionNode) which wraps base node (annotation) searches.
pub struct NodeSearch<'a> {
    /// The actual search implementation
    it: Box<Iterator<Item = Vec<Match>> + 'a>,

    desc: Option<Desc>,
    node_search_desc: Rc<NodeSearchDesc>,
}

#[derive(Clone)]
pub enum NodeSearchSpec {
    ExactValue {
        ns: Option<String>,
        name: String,
        val: Option<String>,
    },
    ExactTokenValue {
        val : String,
        leafs_only : bool,
    },
    AnyToken,
    AnyNode,
}

impl NodeSearchSpec {
    pub fn new_exact(ns : Option<&str>, name : &str, val : Option<&str>) -> NodeSearchSpec {
        NodeSearchSpec::ExactValue {
            ns: ns.map(|v| String::from((v))),
            name : String::from(name),
            val: val.map(|v| String::from(v)),
        }
    }

}

impl fmt::Display for NodeSearchSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &NodeSearchSpec::ExactValue {ref ns, ref name, ref val, .. } =>
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
            },
            &NodeSearchSpec::ExactTokenValue {ref val, ref leafs_only} => {
                if *leafs_only {
                    write!(f, "tok=\"{}\"", val)
                } else {
                    write!(f, "\"{}\"", val)
                }
            },
            &NodeSearchSpec::AnyToken => write!(f, "tok"),
            &NodeSearchSpec::AnyNode => write!(f, "node"),
        }
    }
}

impl<'a> NodeSearch<'a> {

   
    pub fn from_spec(spec: NodeSearchSpec, db: &'a GraphDB) -> Option<NodeSearch<'a>> {

        let query_fragment = format!("{}", spec);

        match spec {
            NodeSearchSpec::ExactValue {ns, name, val } => NodeSearch::from_annosearch(db, ns, name, val, false, &query_fragment),
            NodeSearchSpec::ExactTokenValue {val , leafs_only } => {
                 
                let val_id = db.strings.find_id(&val)?.clone();
                let tok_key = db.get_token_key();

                let any_anno = Annotation{key: db.get_node_type_key(), val: 0};

                let cov_gs = db.get_graphstorage(&Component { ctype: ComponentType::Coverage, layer: String::from(ANNIS_NS), name: String::from("")});

                let it : Box<Iterator<Item = Vec<Match>> + 'a> = if leafs_only {
                    Box::new(
                        db.node_annos
                        .exact_anno_search(Some(tok_key.ns), tok_key.name, Some(val_id))
                        .filter(move |n| if let Some(ref cov) = cov_gs {true} else {true})
                        .map(move |n| vec![Match {node: n.node, anno: any_anno.clone()}])
                    )
                } else {
                    Box::new(
                        db.node_annos
                        .exact_anno_search(Some(tok_key.ns), tok_key.name, Some(val_id))
                        .map(move |n| vec![Match {node: n.node, anno: any_anno.clone()}])
                    )
                };

                let filter_func: Box<Fn(Annotation) -> bool> = 
                    Box::new(move |anno: Annotation| {
                        return anno.key.ns == tok_key.ns && anno.key.name == tok_key.name
                            && anno.val == val_id;
                    });

                let tok_key = db.get_token_key();


                return Some(NodeSearch {
                    it,
                    desc: Some(Desc::empty_with_fragment(&query_fragment)),
                    node_search_desc: Rc::new(NodeSearchDesc {
                        qname: (Some(tok_key.ns), Some(tok_key.name)),
                        cond: filter_func,
                    }),
                });
            },
            NodeSearchSpec::AnyToken => {
                let tok_key = db.get_token_key();
                let any_anno = Annotation{key: db.get_node_type_key(), val: 0};
                let it = db.node_annos
                        .exact_anno_search(Some(tok_key.ns), tok_key.name, None)
                        .map(move |n| vec![Match {node: n.node, anno: any_anno.clone()}]);

                let filter_func: Box<Fn(Annotation) -> bool> = 
                    Box::new(move |anno: Annotation| {
                        return anno.key.ns == tok_key.ns && anno.key.name == tok_key.name;
                    });

                let tok_key = db.get_token_key();
                
                Some(NodeSearch {
                    it : Box::new(it),
                    desc: Some(Desc::empty_with_fragment("tok")),
                    node_search_desc : Rc::new(NodeSearchDesc{
                        qname: (Some(tok_key.ns), Some(tok_key.name)),
                        cond: filter_func,
                    })
                })
            },
            NodeSearchSpec::AnyNode => {
                let type_key = db.get_node_type_key();
                let node_id = db.strings.find_id("node")?.clone();
                let it = db.node_annos
                        .exact_anno_search(Some(type_key.ns), type_key.name, None)
                        .map(move |n| vec![n]);

                let filter_func: Box<Fn(Annotation) -> bool> = 
                    Box::new(move |anno: Annotation| {
                        return anno.key.ns == type_key.ns && anno.key.name == type_key.name
                            && anno.val == node_id;
                    });

                let type_key = db.get_node_type_key();
                
                Some(NodeSearch {
                    it : Box::new(it),
                    desc: Some(Desc::empty_with_fragment("tok")),
                    node_search_desc : Rc::new(NodeSearchDesc{
                        qname: (Some(type_key.ns), Some(type_key.name)),
                        cond: filter_func,
                    })
                })
            }
        }
    }


    fn from_annosearch(db: &'a GraphDB, 
        ns: Option<String>,
        name: String,
        val: Option<String>,
        match_regex : bool,
        query_fragment : &str) -> Option<NodeSearch<'a>> {
            
            let name_id: StringID = db.strings.find_id(&name)?.clone();
            // not finding the strings will result in an None result, not in an less specific search
            let ns_id: Option<StringID> = if let Some(ns) = ns.as_ref() {
                Some(db.strings.find_id(ns)?).cloned()
            } else {
                None
            };
            let val_id: Option<StringID> = if let Some(val) = val.as_ref() {
                Some(db.strings.find_id(val)?).cloned()
            } else {
                None
            };

            let it = db.node_annos
                    .exact_anno_search(ns_id, name_id, val_id)
                    .map(|n| vec![n]);
            

            let filter_func: Box<Fn(Annotation) -> bool> = if ns_id.is_some() && val_id.is_some() {
                Box::new(move |anno: Annotation| {
                    return anno.key.ns == ns_id.unwrap() && anno.key.name == name_id
                        && anno.val == val_id.unwrap();
                })
            } else if ns_id.is_some() {
                Box::new(move |anno: Annotation| {
                    return anno.key.ns == ns_id.unwrap() && anno.key.name == name_id;
                })
            } else if val_id.is_some() {
                Box::new(move |anno: Annotation| {
                    return anno.key.name == name_id && anno.val == val_id.unwrap();
                })
            } else {
                Box::new(move |anno: Annotation| return anno.key.name == name_id)
            };

            return Some(NodeSearch {
                it: Box::new(it),
                desc: Some(Desc::empty_with_fragment(&query_fragment)),
                node_search_desc: Rc::new(NodeSearchDesc {
                    qname: (ns_id, Some(name_id)),
                    cond: filter_func,
                }),
            });
    }
    

    pub fn set_desc(&mut self, desc: Option<Desc>) {
        self.desc = desc;
    }


    pub fn get_node_search_desc(&self) -> Rc<NodeSearchDesc> {
        self.node_search_desc.clone()
    }
}

impl<'a> ExecutionNode for NodeSearch<'a> {
    fn as_iter(&mut self) -> &mut Iterator<Item = Vec<Match>> {
        self
    }

    fn get_desc(&self) -> Option<&Desc> {
        self.desc.as_ref()
    }

    fn as_nodesearch(&self) -> Option<&NodeSearch> {
        Some(self)
    }
}

impl<'a> Iterator for NodeSearch<'a> {
    type Item = Vec<Match>;

    fn next(&mut self) -> Option<Vec<Match>> {
        self.it.next()
    }
}
