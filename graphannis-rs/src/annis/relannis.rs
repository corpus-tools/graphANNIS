use graphdb::GraphDB;
use annis::{AnnoKey, Annotation, NodeID, Component, ComponentType, Edge};
use annis::graphstorage::WriteableGraphStorage;
use annis;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;
use std::num::ParseIntError;
use std::collections::BTreeMap;
use multimap::MultiMap;

use std;
use csv;

pub struct RelANNISLoader;

#[derive(Debug)]
pub enum Error {
    IOError(std::io::Error),
    CSVError(csv::Error),
    GraphDBError(annis::graphdb::Error),
    MissingColumn,
    InvalidDataType,
    ToplevelCorpusNameNotFound,
    DirectoryNotFound,
    DocumentMissing,
    Other,
}

type Result<T> = std::result::Result<T, Error>;

impl From<ParseIntError> for Error {
    fn from(_: ParseIntError) -> Error {
        Error::InvalidDataType
    }
}

impl From<csv::Error> for Error {
    fn from(e: csv::Error) -> Error {
        Error::CSVError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IOError(e)
    }
}

impl From<annis::graphdb::Error> for Error {
    fn from(e: annis::graphdb::Error) -> Error {
        Error::GraphDBError(e)
    }
}


#[derive(Eq, PartialEq, PartialOrd, Ord, Hash, Clone)]
struct TextProperty {
    segmentation: String,
    corpus_id: u32,
    text_id: u32,
    val: u32,
}

fn postgresql_import_reader(path: &Path) -> std::result::Result<csv::Reader<File>, csv::Error> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(path)
}

fn parse_corpus_tab(
    path: &PathBuf,
    corpus_by_preorder: &mut BTreeMap<u32, u32>,
    corpus_id_to_name: &mut BTreeMap<u32, String>,
    is_annis_33: bool,
) -> Result<String> {
    let mut corpus_tab_path = PathBuf::from(path);
    corpus_tab_path.push(if is_annis_33 {
        "corpus.annis"
    } else {
        "corpus.tab"
    });

    let mut toplevel_corpus_name: Option<String> = None;

    let mut corpus_tab_csv = postgresql_import_reader(corpus_tab_path.as_path())?;

    for result in corpus_tab_csv.records() {
        let line = result?;

        let id = line.get(0).ok_or(Error::MissingColumn)?.parse::<u32>()?;
        let name = line.get(1).ok_or(Error::MissingColumn)?;
        let type_str = line.get(2).ok_or(Error::MissingColumn)?;
        let pre_order = line.get(4).ok_or(Error::MissingColumn)?.parse::<u32>()?;

        corpus_id_to_name.insert(id, String::from(name));
        if type_str == "CORPUS" && pre_order == 0 {
            toplevel_corpus_name = Some(String::from(name));
            corpus_by_preorder.insert(pre_order, id);
        } else if type_str == "DOCUMENT" {
            // TODO: do not only add documents but also sub-corpora
            corpus_by_preorder.insert(pre_order, id);
        }
    }

    toplevel_corpus_name.ok_or(Error::ToplevelCorpusNameNotFound)
}

fn calculate_automatic_token_info( db: &mut GraphDB,
    token_by_index : &BTreeMap<TextProperty, NodeID>,
    node_to_left: &BTreeMap<NodeID, u32>,
    node_to_right: &BTreeMap<NodeID, u32>,
    left_to_node: &MultiMap<TextProperty, NodeID>,
    right_to_node: &MultiMap<TextProperty, NodeID>
) -> Result<()> {

    info!("calculating the automatically generated ORDERING, LEFT_TOKEN and RIGHT_TOKEN edges");

    let mut last_textprop : Option<TextProperty> = None;
    let mut last_token : Option<NodeID> = None;

    let component_left = Component{ctype: ComponentType::LeftToken, 
            layer: String::from("annis"), name: String::from("")};
    let component_right = Component{ctype: ComponentType::RightToken, 
        layer: String::from("annis"), name: String::from("")};

    for (current_textprop, current_token) in  token_by_index {

        if current_textprop.segmentation == "" {
            // find all nodes that start together with the current token
            let current_token_left = TextProperty{
                segmentation: String::from(""), 
                text_id : current_textprop.text_id, 
                corpus_id : current_textprop.corpus_id,
                val : try!(node_to_left.get(&current_token).ok_or(Error::Other)).clone(),
            };
            let left_aligned = left_to_node.get_vec(&current_token_left);
            if left_aligned.is_some() {
                let gs_left = db.get_or_create_writable(component_left.clone())?;

                for n in left_aligned.unwrap() {
                    gs_left.add_edge(Edge{source: n.clone(), target: current_token.clone()});
                    gs_left.add_edge(Edge{source: current_token.clone(), target: n.clone()});
                }
            }
            // find all nodes that end together with the current token
            let current_token_right = TextProperty{
                segmentation: String::from(""), 
                text_id : current_textprop.text_id, 
                corpus_id : current_textprop.corpus_id,
                val : try!(node_to_right.get(current_token).ok_or(Error::Other)).clone(),
            };
            let right_aligned = right_to_node.get_vec(&current_token_right);
            if right_aligned.is_some() {
                let gs_right = db.get_or_create_writable(component_right.clone())?;
                for n in right_aligned.unwrap() {
                    gs_right.add_edge(Edge{source: n.clone(), target: current_token.clone()});
                    gs_right.add_edge(Edge{source: current_token.clone(), target: n.clone()});
                }
            }
        } // end if current segmentation is default

        let component_order = Component{ctype: ComponentType::Ordering, 
            layer: String::from("annis"), name: current_textprop.segmentation.clone()};

        let gs_order = db.get_or_create_writable(component_order.clone())?;
        
        // if the last token/text value is valid and we are still in the same text
        if  last_token.is_some()
            && last_textprop.is_some()
            && last_textprop.unwrap() == current_textprop.clone() {
            // we are still in the same text, add ordering between token
            gs_order.add_edge(Edge{source: last_token.unwrap(), target: current_token.clone()});
        } // end if same text

        // update the iterator and other variables
        last_textprop = Some(current_textprop.clone());
        last_token = Some(current_token.clone());

    } // end for each token


    Ok(())
}

fn calculate_automatic_coverage_edges(db: &mut GraphDB,
    token_by_index : &BTreeMap<TextProperty, NodeID>,
    token_to_index: &BTreeMap<NodeID, TextProperty>,
    node_to_right: &BTreeMap<NodeID, u32>,
    left_to_node: &MultiMap<TextProperty, NodeID>,
    right_to_node: &MultiMap<TextProperty, NodeID>,
    token_by_left_textpos: &BTreeMap<TextProperty, NodeID>,
    token_by_right_textpos: &BTreeMap<TextProperty, NodeID>,
    ) 
 -> Result<()> {

     // add explicit coverage edges for each node in the special annis namespace coverage component
    let component_coverage = Component{ctype: ComponentType::Coverage, 
                layer: String::from("annis"), name: String::from("")};
    let component_inv_cov = Component{ctype: ComponentType::InverseCoverage, 
                layer: String::from("annis"), name: String::from("")};
    {
        info!("calculating the automatically generated COVERAGE edges");
        for (textprop, n_vec) in left_to_node {
            for n in n_vec {
                if !token_to_index.contains_key(&n) {
                    
                    let left_pos = TextProperty{
                        segmentation : String::from(""),
                        corpus_id : textprop.corpus_id,
                        text_id : textprop.text_id,
                        val : textprop.val,
                    };
                    let right_pos = node_to_right.get(&n).ok_or(Error::Other)?;
                    let right_pos = TextProperty{
                        segmentation : String::from(""),
                        corpus_id : textprop.corpus_id,
                        text_id : textprop.text_id,
                        val : right_pos.clone(),
                    };

                    // find left/right aligned basic token
                    let left_aligned_tok = token_by_left_textpos.get(&left_pos).ok_or(Error::Other)?;
                    let right_aligned_tok = token_by_right_textpos.get(&right_pos).ok_or(Error::Other)?;

                    let left_tok_pos = token_to_index.get(&left_aligned_tok).ok_or(Error::Other)?;
                    let right_tok_pos = token_to_index.get(&right_aligned_tok).ok_or(Error::Other)?;
                    for i in left_tok_pos.val..(right_tok_pos.val+1) {
                        let tok_idx = TextProperty{
                            segmentation : String::from(""),
                            corpus_id : textprop.corpus_id,
                            text_id : textprop.text_id,
                            val : i,
                        };
                        let tok_id = token_by_index.get(&tok_idx).ok_or(Error::Other)?;
                        if n.clone() != tok_id.clone() {
                            {
                                let gs = db.get_or_create_writable(component_coverage.clone())?;
                                gs.add_edge(Edge{source: n.clone(), target: tok_id.clone()});
                            }
                            {
                                let gs = db.get_or_create_writable(component_inv_cov.clone())?;
                                gs.add_edge(Edge{source: tok_id.clone(), target: n.clone()});
                            }
                        }
                    }      
                } // end if not a token
            }
        }            
    }

     Ok(())
 }

fn load_node_tab(path: &PathBuf,
    db: &mut GraphDB,
    nodes_by_corpus_id: &mut MultiMap<u32, NodeID>,
    corpus_id_to_name: &mut BTreeMap<u32, String>,
    missing_seg_span: &mut BTreeMap<NodeID, String>,
    toplevel_corpus_name: &str,
    is_annis_33: bool) 
-> Result<()> {

    let mut node_tab_path = PathBuf::from(path);
    node_tab_path.push(if is_annis_33 {
        "node.annis"
    } else {
        "node.tab"
    });

    info!("loading {}", node_tab_path.to_str().unwrap_or_default());

    // maps a token index to an node ID
    let mut token_by_index: BTreeMap<TextProperty, NodeID> = BTreeMap::new();

    // map the "left" value to the nodes it belongs to
    let mut left_to_node: MultiMap<TextProperty, NodeID> = MultiMap::new();
    // map the "right" value to the nodes it belongs to
    let mut right_to_node: MultiMap<TextProperty, NodeID> = MultiMap::new();

    // map as node to it's "left" value
    let mut node_to_left: BTreeMap<NodeID, u32> = BTreeMap::new();
    // map as node to it's "right" value
    let mut node_to_right: BTreeMap<NodeID, u32> = BTreeMap::new();

    // maps a character position to it's token
    let mut token_by_left_textpos: BTreeMap<TextProperty, NodeID> = BTreeMap::new();
    let mut token_by_right_textpos: BTreeMap<TextProperty, NodeID> = BTreeMap::new();

    // maps a token node id to the token index
    let mut token_to_index: BTreeMap<NodeID, TextProperty> = BTreeMap::new();

    // start "scan all lines" visibility block
    {
        let mut node_tab_csv = postgresql_import_reader(node_tab_path.as_path())?;

        for result in node_tab_csv.records() {
            let line = result?;

            let node_nr = line.get(0).ok_or(Error::MissingColumn)?.parse::<NodeID>()?;
            let has_segmentations = is_annis_33 || line.len() > 10;
            let token_index_raw = line.get(7).ok_or(Error::MissingColumn)?;
            let text_id = line.get(1).ok_or(Error::MissingColumn)?.parse::<u32>()?;
            let corpus_id = line.get(2).ok_or(Error::MissingColumn)?.parse::<u32>()?;
            let layer: &str = line.get(3).ok_or(Error::MissingColumn)?;
            let node_name = line.get(4).ok_or(Error::MissingColumn)?;

            let doc_name = corpus_id_to_name
                .get(&corpus_id)
                .ok_or(Error::DocumentMissing)?;
            nodes_by_corpus_id.insert(corpus_id, node_nr);

            let node_qname = format!("{}/{}#{}", toplevel_corpus_name, doc_name, node_name);
            let node_name_anno = Annotation {
                key: db.get_node_name_key(),
                val: db.strings.add(&node_qname),
            };
            db.node_annos.insert(node_nr, node_name_anno);

            let node_type_anno = Annotation {
                key: db.get_node_type_key(),
                val: db.strings.add("node"),
            };
            db.node_annos.insert(node_nr, node_type_anno);

            if !layer.is_empty() && layer != "NULL" {
                let layer_anno = Annotation {
                    key: AnnoKey {
                        ns: db.strings.add("annis"),
                        name: db.strings.add("layer"),
                    },
                    val: db.strings.add(layer),
                };
                db.node_annos.insert(node_nr, layer_anno);
            }

            let left_val = line.get(5).ok_or(Error::MissingColumn)?.parse::<u32>()?;
            let left = TextProperty {
                segmentation: String::from(""),
                val: left_val,
                corpus_id,
                text_id,
            };
            let right_val = line.get(6).ok_or(Error::MissingColumn)?.parse::<u32>()?;
            let right = TextProperty {
                segmentation: String::from(""),
                val: right_val,
                corpus_id,
                text_id,
            };
            left_to_node.insert(left.clone(), node_nr);
            right_to_node.insert(right.clone(), node_nr);
            node_to_left.insert(node_nr, left_val);
            node_to_right.insert(node_nr, right_val);

            if token_index_raw != "NULL" {
                let span = if has_segmentations {
                    line.get(12).ok_or(Error::MissingColumn)?
                } else {
                    line.get(9).ok_or(Error::MissingColumn)?
                };

                let tok_anno = Annotation{
                    key : db.get_token_key(),
                    val : db.strings.add(span),
                };
                db.node_annos.insert(node_nr, tok_anno);

                let index = TextProperty {
                    segmentation : String::from(""),
                    val : token_index_raw.parse::<u32>()?,
                    text_id,
                    corpus_id,
                };
                token_by_index.insert(index.clone(), node_nr);
                token_to_index.insert(node_nr, index);
                token_by_left_textpos.insert(left, node_nr);
                token_by_right_textpos.insert(right, node_nr);

            } else if has_segmentations {
                let segmentation_name = if is_annis_33 {
                    line.get(11).ok_or(Error::MissingColumn)?
                } else {
                    line.get(8).ok_or(Error::MissingColumn)?
                };

                if segmentation_name != "NULL" {
                    let seg_index = if is_annis_33 {
                        line.get(10).ok_or(Error::MissingColumn)?.parse::<u32>()?
                    } else {
                        line.get(9).ok_or(Error::MissingColumn)?.parse::<u32>()?
                    };

                    if is_annis_33 {
                        // directly add the span information
                        let tok_anno = Annotation {
                            key : db.get_token_key(),
                            val : db.strings.add(line.get(12).ok_or(Error::MissingColumn)?),
                        };
                        db.node_annos.insert(node_nr, tok_anno);
                    } else {
                        // we need to get the span information from the node_annotation file later
                        missing_seg_span.insert(node_nr, String::from(segmentation_name));
                    }
                    // also add the specific segmentation index
                    let index = TextProperty {
                        segmentation : String::from(segmentation_name),
                        val : seg_index,
                        corpus_id,
                        text_id,
                    };
                    token_by_index.insert(index, node_nr);

                } // end if node has segmentation info

            } // endif if check segmentations
        }
    } // end "scan all lines" visibility block

    // TODO: cleanup, better variable naming and put this into it's own function
    // iterate over all token by their order, find the nodes with the same
    // text coverage (either left or right) and add explicit ORDERING, LEFT_TOKEN and RIGHT_TOKEN edges
    if !token_by_index.is_empty() {
        calculate_automatic_token_info(db, &token_by_index, &node_to_left, &node_to_right, &left_to_node, &right_to_node)?;
    } // end if token_by_index not empty

    calculate_automatic_coverage_edges(db, &token_by_index, &token_to_index, 
        &node_to_right, &left_to_node, &right_to_node, &token_by_left_textpos, &token_by_right_textpos)?;
    Ok(())
}

fn load_node_anno_tab() -> Result<()> {
    unimplemented!();
}

fn load_nodes(
    path: &PathBuf,
    db: &mut GraphDB,
    nodes_by_corpus_id: &mut MultiMap<u32, NodeID>,
    corpus_id_to_name: &mut BTreeMap<u32, String>,
    toplevel_corpus_name: &str,
    is_annis_33: bool,
) -> Result<()> {

    let mut missing_seg_span: BTreeMap<NodeID, String> = BTreeMap::new();
    
    load_node_tab(path, db, nodes_by_corpus_id, corpus_id_to_name, &mut missing_seg_span, toplevel_corpus_name, is_annis_33)?;


    return Ok(());
}



pub fn load(path: &str) -> Result<GraphDB> {
    // convert to path
    let mut path = PathBuf::from(path);
    if path.is_dir() && path.exists() {
        // check if this is the ANNIS 3.3 import format
        path.push("annis.version");
        let mut is_annis_33 = false;
        if path.exists() {
            let mut file = File::open(&path)?;
            let mut version_str = String::new();
            file.read_to_string(&mut version_str)?;

            is_annis_33 = version_str == "3.3";
        }

        let mut db = GraphDB::new();

        let mut corpus_by_preorder = BTreeMap::new();
        let mut corpus_id_to_name = BTreeMap::new();
        let mut nodes_by_corpus_id: MultiMap<u32, NodeID> = MultiMap::new();
        let corpus_name = parse_corpus_tab(
            &path,
            &mut corpus_by_preorder,
            &mut corpus_id_to_name,
            is_annis_33,
        )?;

        load_nodes(
            &path,
            &mut db,
            &mut nodes_by_corpus_id,
            &mut corpus_id_to_name,
            &corpus_name,
            is_annis_33,
        )?;


        return Ok(db);
    }

    return Err(Error::DirectoryNotFound);
}
