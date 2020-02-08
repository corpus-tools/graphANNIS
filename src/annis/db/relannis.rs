use crate::annis::db::corpusstorage::SALT_URI_ENCODE_SET;
use crate::annis::db::{Graph, ANNIS_NS, TOK};
use crate::annis::errors::*;
use crate::annis::types::{AnnoKey, Component, ComponentType, Edge, NodeID};
use crate::annis::util::disk_collections::{DiskMap, KeySerializer};
use crate::annis::util::{create_str_vec_key, parse_str_vec_key};
use crate::update::{GraphUpdate, UpdateEvent};
use csv;
use percent_encoding::utf8_percent_encode;
use std;
use std::collections::{BTreeMap, HashMap};
use std::convert::TryInto;
use std::fs::File;
use std::io::prelude::*;
use std::ops::Bound::Included;
use std::path::{Path, PathBuf};

#[derive(
    Eq, PartialEq, PartialOrd, Ord, Hash, Clone, Debug, Serialize, Deserialize, MallocSizeOf,
)]
struct TextProperty {
    segmentation: String,
    corpus_id: u32,
    text_id: u32,
    val: u32,
}

impl KeySerializer for TextProperty {
    fn create_key(&self) -> Vec<u8> {
        let mut result =
            Vec::with_capacity(self.segmentation.len() + 1 + std::mem::size_of::<u32>() * 3);
        result.extend(create_str_vec_key(&[&self.segmentation]));
        result.extend(&self.corpus_id.to_be_bytes());
        result.extend(&self.text_id.to_be_bytes());
        result.extend(&self.val.to_be_bytes());
        result
    }

    fn parse_key(key: &[u8]) -> Self {
        let id_size = std::mem::size_of::<u32>();
        let mut id_offset = key.len() - id_size * 3;
        let segmentation_vector = parse_str_vec_key(&key[0..id_offset]);

        let corpus_id = u32::from_be_bytes(
            key[id_offset..(id_offset + id_size)]
                .try_into()
                .expect("TextProperty deserialization key was too small"),
        );
        id_offset += id_size;

        let text_id = u32::from_be_bytes(
            key[id_offset..(id_offset + id_size)]
                .try_into()
                .expect("TextProperty deserialization key was too small"),
        );
        id_offset += id_size;

        let val = u32::from_be_bytes(
            key[id_offset..(id_offset + id_size)]
                .try_into()
                .expect("TextProperty deserialization key was too small"),
        );

        let segmentation = if segmentation_vector.is_empty() {
            String::from("")
        } else {
            segmentation_vector[0].to_string()
        };

        TextProperty {
            segmentation,
            corpus_id,
            text_id,
            val,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    id: u32,
    corpus_ref: Option<u32>,
}

#[derive(Clone, PartialEq, PartialOrd, Eq, Ord, MallocSizeOf)]
struct NodeByTextEntry {
    text_id: u32,
    corpus_ref: u32,
    node_id: NodeID,
}

impl KeySerializer for NodeByTextEntry {
    fn create_key(&self) -> Vec<u8> {
        let mut result =
            Vec::with_capacity(std::mem::size_of::<u32>() * 2 + std::mem::size_of::<NodeID>());
        result.extend(&self.text_id.to_be_bytes());
        result.extend(&self.corpus_ref.to_be_bytes());
        result.extend(&self.node_id.to_be_bytes());
        result
    }

    fn parse_key(key: &[u8]) -> Self {
        let u32_size = std::mem::size_of::<u32>();
        let text_id = u32::from_be_bytes(
            key[0..u32_size]
                .try_into()
                .expect("NodeByTextEntry deserialization key was too small (text_id)"),
        );
        let mut offset = u32_size;

        let corpus_ref = u32::from_be_bytes(
            key[offset..(offset + u32_size)]
                .try_into()
                .expect("NodeByTextEntry deserialization key was too small (corpus_ref)"),
        );
        offset += u32_size;

        let node_id = NodeID::from_be_bytes(
            key[offset..]
                .try_into()
                .expect("NodeByTextEntry deserialization key was too small (node_id)"),
        );

        NodeByTextEntry {
            text_id,
            corpus_ref,
            node_id,
        }
    }
}

struct Text {
    name: String,
}

struct CorpusTableEntry {
    pre: u32,
    post: u32,
    name: String,
}

struct ParsedCorpusTable {
    toplevel_corpus_name: String,
    corpus_by_preorder: BTreeMap<u32, u32>,
    corpus_by_id: BTreeMap<u32, CorpusTableEntry>,
}

struct TextPosTable {
    token_by_left_textpos: DiskMap<TextProperty, NodeID>,
    token_by_right_textpos: DiskMap<TextProperty, NodeID>,
    // maps a token index to an node ID
    token_by_index: DiskMap<TextProperty, NodeID>,
    // maps a token node id to the token index
    token_to_index: DiskMap<NodeID, TextProperty>,
    // map as node to it's "left" value
    node_to_left: DiskMap<NodeID, TextProperty>,
    // map as node to it's "right" value
    node_to_right: DiskMap<NodeID, TextProperty>,
}

struct LoadRankResult {
    components_by_pre: DiskMap<u32, Component>,
    edges_by_pre: DiskMap<u32, Edge>,
    text_coverage_edges: DiskMap<Edge, bool>,
}

struct LoadNodeAndCorpusResult {
    toplevel_corpus_name: String,
    id_to_node_name: DiskMap<NodeID, String>,
    textpos_table: TextPosTable,
}

struct NodeTabParseResult {
    nodes_by_text: DiskMap<NodeByTextEntry, bool>,
    missing_seg_span: BTreeMap<NodeID, String>,
    id_to_node_name: DiskMap<NodeID, String>,
    textpos_table: TextPosTable,
}

struct LoadNodeResult {
    nodes_by_text: DiskMap<NodeByTextEntry, bool>,
    id_to_node_name: DiskMap<NodeID, String>,
    textpos_table: TextPosTable,
}

struct ChunkUpdater<'a> {
    g: &'a mut Graph,
    max_number_events: usize,
    update: GraphUpdate,
    num_of_commits: usize,
}

impl<'a> ChunkUpdater<'a> {
    fn new(g: &'a mut Graph, max_number_events: usize) -> ChunkUpdater<'a> {
        ChunkUpdater {
            g,
            max_number_events,
            update: GraphUpdate::new(),
            num_of_commits: 0,
        }
    }

    fn add_event<F>(
        &mut self,
        event: UpdateEvent,
        message: Option<&str>,
        progress_callback: &F,
    ) -> Result<()>
    where
        F: Fn(&str),
    {
        self.update.add_event(event);

        if self.update.len() >= self.max_number_events {
            self.commit(message, progress_callback)?;
        }

        Ok(())
    }

    fn commit<F>(&mut self, message: Option<&str>, progress_callback: &F) -> Result<()>
    where
        F: Fn(&str),
    {
        self.num_of_commits += 1;
        if let Some(message) = message {
            progress_callback(&format!(
                "{} ({} updates, {} commits)",
                message,
                self.update.len(),
                self.num_of_commits
            ));
        } else {
            progress_callback(&format!(
                "committing {} updates ({} commits)",
                self.update.len(),
                self.num_of_commits
            ));
        }
        self.g.apply_update(&mut self.update)?;
        self.update = GraphUpdate::new();
        Ok(())
    }
}

/// Load a c corpus in the legacy relANNIS format from the specified `path`.
///
/// Returns a tuple consisting of the corpus name and the extracted annotation graph.
pub fn load<F>(path: &Path, disk_based: bool, progress_callback: F) -> Result<(String, Graph)>
where
    F: Fn(&str) -> (),
{
    // convert to path
    let path = PathBuf::from(path);
    if path.is_dir() && path.exists() {
        // check if this is the ANNIS 3.3 import format
        let annis_version_path = path.clone().join("annis.version");
        let is_annis_33 = if annis_version_path.exists() {
            let mut file = File::open(&annis_version_path)?;
            let mut version_str = String::new();
            file.read_to_string(&mut version_str)?;

            version_str == "3.3"
        } else {
            false
        };

        let mut db = Graph::with_default_graphstorages(disk_based)?;
        let mut updater = ChunkUpdater::new(&mut db, 1_000_000);
        let load_node_and_corpus_result =
            load_node_and_corpus_tables(&path, &mut updater, is_annis_33, &progress_callback)?;
        {
            let text_coverage_edges = load_edge_tables(
                &path,
                &mut updater,
                is_annis_33,
                &load_node_and_corpus_result.id_to_node_name,
                &progress_callback,
            )?;

            calculate_automatic_coverage_edges(
                &mut updater,
                &load_node_and_corpus_result,
                &text_coverage_edges,
                &progress_callback,
            )?;
        }

        progress_callback("calculating node statistics");
        db.node_annos.calculate_statistics();

        for c in db.get_all_components(None, None) {
            progress_callback(&format!("calculating statistics for component {}", c));
            db.calculate_component_statistics(&c)?;
            db.optimize_impl(&c)?;
        }

        progress_callback(&format!(
            "finished loading relANNIS from {}",
            path.to_string_lossy()
        ));

        return Ok((load_node_and_corpus_result.toplevel_corpus_name, db));
    }

    Err(format!("Directory {} not found", path.to_string_lossy()).into())
}

fn load_node_and_corpus_tables<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<LoadNodeAndCorpusResult>
where
    F: Fn(&str) -> (),
{
    let corpus_table = parse_corpus_tab(&path, is_annis_33, &progress_callback)?;
    let mut texts = parse_text_tab(&path, is_annis_33, &progress_callback)?;
    let corpus_id_to_annos = load_corpus_annotation(&path, is_annis_33, &progress_callback)?;

    let load_nodes_result = load_nodes(
        path,
        updater,
        &mut texts,
        &corpus_table,
        is_annis_33,
        progress_callback,
    )?;

    add_subcorpora(
        updater,
        &corpus_table,
        &load_nodes_result,
        &texts,
        &corpus_id_to_annos,
        is_annis_33,
        progress_callback,
    )?;

    Ok(LoadNodeAndCorpusResult {
        toplevel_corpus_name: corpus_table.toplevel_corpus_name,
        id_to_node_name: load_nodes_result.id_to_node_name,
        textpos_table: load_nodes_result.textpos_table,
    })
}

fn load_edge_tables<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    is_annis_33: bool,
    id_to_node_name: &DiskMap<NodeID, String>,
    progress_callback: &F,
) -> Result<LoadRankResult>
where
    F: Fn(&str) -> (),
{
    let load_rank_result = {
        let component_by_id = load_component_tab(path, is_annis_33, progress_callback)?;

        load_rank_tab(
            path,
            updater,
            &component_by_id,
            id_to_node_name,
            is_annis_33,
            progress_callback,
        )?
    };

    load_edge_annotation(
        path,
        updater,
        &load_rank_result,
        id_to_node_name,
        is_annis_33,
        progress_callback,
    )?;

    Ok(load_rank_result)
}

fn postgresql_import_reader(path: &Path) -> std::result::Result<csv::Reader<File>, csv::Error> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .quote(0) // effectivly disable quoting
        .from_path(path)
}

fn get_field_str(record: &csv::StringRecord, i: usize) -> Option<String> {
    if let Some(r) = record.get(i) {
        // replace some known escape sequences
        return Some(
            r.replace("\\t", "\t")
                .replace("\\'", "'")
                .replace("\\\\", "\\"),
        );
    }
    None
}

fn parse_corpus_tab<F>(
    path: &PathBuf,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<ParsedCorpusTable>
where
    F: Fn(&str) -> (),
{
    let mut corpus_tab_path = PathBuf::from(path);
    corpus_tab_path.push(if is_annis_33 {
        "corpus.annis"
    } else {
        "corpus.tab"
    });

    progress_callback(&format!(
        "loading {}",
        corpus_tab_path.to_str().unwrap_or_default()
    ));

    let mut corpus_by_preorder = BTreeMap::new();
    let mut corpus_by_id = BTreeMap::new();

    let mut corpus_tab_csv = postgresql_import_reader(corpus_tab_path.as_path())?;

    for result in corpus_tab_csv.records() {
        let line = result?;

        let id = line.get(0).ok_or("Missing column")?.parse::<u32>()?;
        let name = get_field_str(&line, 1).ok_or("Missing column")?;
        let pre_order = line.get(4).ok_or("Missing column")?.parse::<u32>()?;
        let post_order = line.get(5).ok_or("Missing column")?.parse::<u32>()?;

        corpus_by_id.insert(
            id,
            CorpusTableEntry {
                pre: pre_order,
                post: post_order,
                name: name.clone(),
            },
        );

        corpus_by_preorder.insert(pre_order, id);
    }

    let toplevel_corpus_id = corpus_by_preorder
        .iter()
        .next()
        .ok_or("Toplevel corpus not found")?
        .1;
    Ok(ParsedCorpusTable {
        toplevel_corpus_name: corpus_by_id
            .get(toplevel_corpus_id)
            .ok_or("Toplevel corpus name not found")?
            .name
            .clone(),
        corpus_by_preorder,
        corpus_by_id,
    })
}

fn parse_text_tab<F>(
    path: &PathBuf,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<HashMap<TextKey, Text>>
where
    F: Fn(&str) -> (),
{
    let mut text_tab_path = PathBuf::from(path);
    text_tab_path.push(if is_annis_33 {
        "text.annis"
    } else {
        "text.tab"
    });

    progress_callback(&format!(
        "loading {}",
        text_tab_path.to_str().unwrap_or_default()
    ));

    let mut texts: HashMap<TextKey, Text> = HashMap::default();

    let mut text_tab_csv = postgresql_import_reader(text_tab_path.as_path())?;

    for result in text_tab_csv.records() {
        let line = result?;

        let id = line
            .get(if is_annis_33 { 1 } else { 0 })
            .ok_or("Missing column")?
            .parse::<u32>()?;
        let name = get_field_str(&line, if is_annis_33 { 2 } else { 1 }).ok_or("Missing column")?;

        let corpus_ref = if is_annis_33 {
            Some(line.get(0).ok_or("Missing column")?.parse::<u32>()?)
        } else {
            None
        };
        let key = TextKey { id, corpus_ref };
        texts.insert(key.clone(), Text { name });
    }

    Ok(texts)
}

fn calculate_automatic_token_order<F>(
    updater: &mut ChunkUpdater,
    token_by_index: &DiskMap<TextProperty, NodeID>,
    id_to_node_name: &DiskMap<NodeID, String>,
    progress_callback: &F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    // TODO: cleanup, better variable naming
    // iterate over all token by their order, find the nodes with the same
    // text coverage (either left or right) and add explicit Ordering edge

    let msg = "calculating the automatically generated Ordering edges";
    progress_callback(msg);

    let mut last_textprop: Option<TextProperty> = None;
    let mut last_token: Option<NodeID> = None;

    for (current_textprop, current_token) in token_by_index.iter()? {
        // if the last token/text value is valid and we are still in the same text
        if let (Some(last_token), Some(last_textprop)) = (last_token, last_textprop) {
            if last_textprop.corpus_id == current_textprop.corpus_id
                && last_textprop.text_id == current_textprop.text_id
                && last_textprop.segmentation == current_textprop.segmentation
            {
                // we are still in the same text, add ordering between token
                updater.add_event(
                    UpdateEvent::AddEdge {
                        source_node: id_to_node_name
                            .get(&last_token)?
                            .ok_or("Missing node name")?
                            .clone(),
                        target_node: id_to_node_name
                            .get(&current_token)?
                            .ok_or("Missing node name")?
                            .clone(),
                        layer: ANNIS_NS.to_owned(),
                        component_type: ComponentType::Ordering.to_string(),
                        component_name: current_textprop.segmentation.clone(),
                    },
                    Some(msg),
                    progress_callback,
                )?;
            }
        } // end if same text

        // update the iterator and other variables
        last_textprop = Some(current_textprop.clone());
        last_token = Some(current_token);
    } // end for each token

    updater.commit(Some(msg), progress_callback)?;

    Ok(())
}

fn add_automatic_cov_edge_for_node<F>(
    updater: &mut ChunkUpdater,
    n: NodeID,
    left_pos: TextProperty,
    right_pos: TextProperty,
    load_node_and_corpus_result: &LoadNodeAndCorpusResult,
    load_rank_result: &LoadRankResult,
    progress_callback: &F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    // find left/right aligned basic token
    let left_aligned_tok = load_node_and_corpus_result
        .textpos_table
        .token_by_left_textpos
        .get(&left_pos)?
        .ok_or_else(|| format!("Can't get left-aligned token for node {}", n,));
    let right_aligned_tok = load_node_and_corpus_result
        .textpos_table
        .token_by_right_textpos
        .get(&right_pos)?
        .ok_or_else(|| format!("Can't get right-aligned token for node {}", n,));

    // If only one of the aligned token is missing, use it for both sides, this is consistent with
    // the relANNIS import of ANNIS3
    let left_aligned_tok = if let Ok(left_aligned_tok) = left_aligned_tok {
        left_aligned_tok
    } else {
        right_aligned_tok.clone()?
    };
    let right_aligned_tok = if let Ok(right_aligned_tok) = right_aligned_tok {
        right_aligned_tok
    } else {
        left_aligned_tok
    };

    let left_tok_pos = load_node_and_corpus_result
        .textpos_table
        .token_to_index
        .get(&left_aligned_tok)?
        .ok_or_else(|| {
            format!(
                "Can't get position of left-aligned token {}",
                left_aligned_tok
            )
        })?;
    let right_tok_pos = load_node_and_corpus_result
        .textpos_table
        .token_to_index
        .get(&right_aligned_tok)?
        .ok_or_else(|| {
            format!(
                "Can't get position of right-aligned token {}",
                right_aligned_tok
            )
        })?;

    for i in left_tok_pos.val..=right_tok_pos.val {
        let tok_idx = TextProperty {
            segmentation: String::default(),
            corpus_id: left_tok_pos.corpus_id,
            text_id: left_tok_pos.text_id,
            val: i,
        };
        let tok_id = load_node_and_corpus_result
            .textpos_table
            .token_by_index
            .get(&tok_idx)?
            .ok_or_else(|| format!("Can't get token ID for position {:?}", tok_idx))?;
        if n != tok_id {
            let edge = Edge {
                source: n,
                target: tok_id,
            };

            // only add edge of no other coverage edge exists
            if !load_rank_result.text_coverage_edges.get(&edge)?.is_some() {
                let nodes_with_same_source = (
                    Included(Edge {
                        source: n,
                        target: NodeID::min_value(),
                    }),
                    Included(Edge {
                        source: n,
                        target: NodeID::max_value(),
                    }),
                );
                let has_outgoing_text_coverage_edge = load_rank_result
                    .text_coverage_edges
                    .range(nodes_with_same_source)?
                    .next()
                    .is_some();
                let component_name = if has_outgoing_text_coverage_edge {
                    // this is an additional auto-generated coverage edge, mark it as such
                    "autogenerated-coverage"
                } else {
                    // the node has no other coverage edges, use a neutral component
                    ""
                };

                updater.add_event(
                    UpdateEvent::AddEdge {
                        source_node: load_node_and_corpus_result
                            .id_to_node_name
                            .get(&n)?
                            .ok_or("Missing node name")?
                            .clone(),
                        target_node: load_node_and_corpus_result
                            .id_to_node_name
                            .get(&tok_id)?
                            .ok_or("Missing node name")?
                            .clone(),
                        layer: ANNIS_NS.to_owned(),
                        component_type: ComponentType::Coverage.to_string(),
                        component_name: component_name.to_owned(),
                    },
                    Some("calculating the automatically generated Coverage edges"),
                    progress_callback,
                )?;
            }
        }
    }

    Ok(())
}

fn calculate_automatic_coverage_edges<F>(
    updater: &mut ChunkUpdater,
    load_node_and_corpus_result: &LoadNodeAndCorpusResult,
    load_rank_result: &LoadRankResult,
    progress_callback: &F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    // add explicit coverage edges for each node in the special annis namespace coverage component
    progress_callback("calculating the automatically generated Coverage edges");

    for (n, textprop) in load_node_and_corpus_result
        .textpos_table
        .node_to_left
        .iter()?
    {
        if textprop.segmentation == ""
            && !load_node_and_corpus_result
                .textpos_table
                .token_to_index
                .contains_key(&n)?
        {
            let left_pos = TextProperty {
                segmentation: String::from(""),
                corpus_id: textprop.corpus_id,
                text_id: textprop.text_id,
                val: textprop.val,
            };
            let right_pos = load_node_and_corpus_result
                .textpos_table
                .node_to_right
                .get(&n)?
                .ok_or_else(|| format!("Can't get right position of node {}", n))?;
            let right_pos = TextProperty {
                segmentation: String::from(""),
                corpus_id: textprop.corpus_id,
                text_id: textprop.text_id,
                val: right_pos.val,
            };

            if let Err(e) = add_automatic_cov_edge_for_node(
                updater,
                n,
                left_pos,
                right_pos,
                &load_node_and_corpus_result,
                &load_rank_result,
                progress_callback,
            ) {
                // output a warning but do not fail
                warn!(
                    "Adding coverage edges (connects spans with tokens) failed: {}",
                    e
                )
            }
        } // end if not a token
    }

    updater.commit(
        Some("calculating the automatically generated Coverage edges"),
        progress_callback,
    )?;

    Ok(())
}

fn load_node_tab<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    texts: &mut HashMap<TextKey, Text>,
    corpus_table: &ParsedCorpusTable,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<NodeTabParseResult>
where
    F: Fn(&str) -> (),
{
    let mut nodes_by_text: DiskMap<NodeByTextEntry, bool> = DiskMap::default();
    let mut missing_seg_span: BTreeMap<NodeID, String> = BTreeMap::new();
    let mut id_to_node_name: DiskMap<NodeID, String> = DiskMap::default();

    let mut node_tab_path = PathBuf::from(path);
    node_tab_path.push(if is_annis_33 {
        "node.annis"
    } else {
        "node.tab"
    });

    progress_callback(&format!(
        "loading {}",
        node_tab_path.to_str().unwrap_or_default()
    ));

    let msg = "committing nodes";

    // maps a character position to it's token
    let mut textpos_table = TextPosTable {
        token_by_left_textpos: DiskMap::default(),
        token_by_right_textpos: DiskMap::default(),
        node_to_left: DiskMap::default(),
        node_to_right: DiskMap::default(),
        token_by_index: DiskMap::default(),
        token_to_index: DiskMap::default(),
    };

    // start "scan all lines" visibility block
    {
        let mut node_tab_csv = postgresql_import_reader(node_tab_path.as_path())?;

        for result in node_tab_csv.records() {
            let line = result?;

            let node_nr = line.get(0).ok_or("Missing column")?.parse::<NodeID>()?;
            let has_segmentations = is_annis_33 || line.len() > 10;
            let token_index_raw = line.get(7).ok_or("Missing column")?;
            let text_id = line.get(1).ok_or("Missing column")?.parse::<u32>()?;
            let corpus_id = line.get(2).ok_or("Missing column")?.parse::<u32>()?;
            let layer = get_field_str(&line, 3).ok_or("Missing column")?;
            let node_name = get_field_str(&line, 4).ok_or("Missing column")?;

            nodes_by_text.insert(
                NodeByTextEntry {
                    corpus_ref: corpus_id,
                    text_id,
                    node_id: node_nr,
                },
                true,
            )?;

            // complete the corpus reference in the texts map for older relANNIS versions
            if !is_annis_33 {
                let text_key_without_corpus = TextKey {
                    id: text_id,
                    corpus_ref: None,
                };
                if let Some(existing_text) = texts.remove(&text_key_without_corpus) {
                    let text_key = TextKey {
                        id: text_id,
                        corpus_ref: Some(corpus_id),
                    };
                    texts.insert(text_key, existing_text);
                }
            }

            let node_path = format!(
                "{}#{}",
                get_corpus_path(corpus_id, corpus_table)?,
                // fragments don't need escaping
                node_name
            );
            updater.add_event(
                UpdateEvent::AddNode {
                    node_name: node_path.clone(),
                    node_type: "node".to_owned(),
                },
                Some(msg),
                progress_callback,
            )?;
            id_to_node_name.insert(node_nr, node_path.clone())?;

            if !layer.is_empty() && layer != "NULL" {
                updater.add_event(
                    UpdateEvent::AddNodeLabel {
                        node_name: node_path.clone(),
                        anno_ns: ANNIS_NS.to_owned(),
                        anno_name: "layer".to_owned(),
                        anno_value: layer,
                    },
                    Some(msg),
                    progress_callback,
                )?;
            }

            // Use left/right token columns for relANNIS 3.3 and the left/right character column otherwise.
            // For some malformed corpora, the token coverage information is more robust and guaranties that a node is
            // only left/right aligned to a single token.
            let left_column = if is_annis_33 { 8 } else { 5 };
            let right_column = if is_annis_33 { 9 } else { 6 };

            let left_val = line
                .get(left_column)
                .ok_or("Missing column")?
                .parse::<u32>()?;
            let left = TextProperty {
                segmentation: String::from(""),
                val: left_val,
                corpus_id,
                text_id,
            };
            let right_val = line
                .get(right_column)
                .ok_or("Missing column")?
                .parse::<u32>()?;
            let right = TextProperty {
                segmentation: String::from(""),
                val: right_val,
                corpus_id,
                text_id,
            };
            textpos_table.node_to_left.insert(node_nr, left.clone())?;
            textpos_table.node_to_right.insert(node_nr, right.clone())?;

            if token_index_raw != "NULL" {
                let span = if has_segmentations {
                    get_field_str(&line, 12).ok_or("Missing column")?
                } else {
                    get_field_str(&line, 9).ok_or("Missing column")?
                };

                updater.add_event(
                    UpdateEvent::AddNodeLabel {
                        node_name: node_path,
                        anno_ns: ANNIS_NS.to_owned(),
                        anno_name: TOK.to_owned(),
                        anno_value: span,
                    },
                    Some(msg),
                    progress_callback,
                )?;

                let index = TextProperty {
                    segmentation: String::from(""),
                    val: token_index_raw.parse::<u32>()?,
                    text_id,
                    corpus_id,
                };
                textpos_table
                    .token_by_index
                    .insert(index.clone(), node_nr)?;
                textpos_table.token_to_index.insert(node_nr, index)?;
                textpos_table.token_by_left_textpos.insert(left, node_nr)?;
                textpos_table
                    .token_by_right_textpos
                    .insert(right, node_nr)?;
            } else if has_segmentations {
                let segmentation_name = if is_annis_33 {
                    get_field_str(&line, 11).ok_or("Missing column")?
                } else {
                    get_field_str(&line, 8).ok_or("Missing column")?
                };

                if segmentation_name != "NULL" {
                    let seg_index = if is_annis_33 {
                        line.get(10).ok_or("Missing column")?.parse::<u32>()?
                    } else {
                        line.get(9).ok_or("Missing column")?.parse::<u32>()?
                    };

                    if is_annis_33 {
                        // directly add the span information
                        updater.add_event(
                            UpdateEvent::AddNodeLabel {
                                node_name: node_path,
                                anno_ns: ANNIS_NS.to_owned(),
                                anno_name: TOK.to_owned(),
                                anno_value: get_field_str(&line, 12).ok_or("Missing column")?,
                            },
                            Some(msg),
                            progress_callback,
                        )?;
                    } else {
                        // we need to get the span information from the node_annotation file later
                        missing_seg_span.insert(node_nr, segmentation_name.clone());
                    }
                    // also add the specific segmentation index
                    let index = TextProperty {
                        segmentation: segmentation_name,
                        val: seg_index,
                        corpus_id,
                        text_id,
                    };
                    textpos_table.token_by_index.insert(index, node_nr)?;
                } // end if node has segmentation info
            } // endif if check segmentations
        }
    } // end "scan all lines" visibility block

    info!(
        "creating index for content of {}",
        &node_tab_path.to_string_lossy()
    );
    id_to_node_name.compact_and_flush()?;
    nodes_by_text.compact_and_flush()?;
    textpos_table.node_to_left.compact_and_flush()?;
    textpos_table.node_to_right.compact_and_flush()?;
    textpos_table.token_to_index.compact_and_flush()?;
    textpos_table.token_by_index.compact_and_flush()?;
    textpos_table.token_by_left_textpos.compact_and_flush()?;
    textpos_table.token_by_right_textpos.compact_and_flush()?;

    updater.commit(Some(msg), progress_callback)?;

    if !(textpos_table.token_by_index.is_empty())? {
        calculate_automatic_token_order(
            updater,
            &textpos_table.token_by_index,
            &id_to_node_name,
            progress_callback,
        )?;
    } // end if token_by_index not empty

    Ok(NodeTabParseResult {
        nodes_by_text,
        missing_seg_span,
        id_to_node_name,
        textpos_table,
    })
}

fn load_node_anno_tab<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    missing_seg_span: &BTreeMap<NodeID, String>,
    id_to_node_name: &DiskMap<NodeID, String>,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    let mut node_anno_tab_path = PathBuf::from(path);
    node_anno_tab_path.push(if is_annis_33 {
        "node_annotation.annis"
    } else {
        "node_annotation.tab"
    });

    progress_callback(&format!(
        "loading {}",
        node_anno_tab_path.to_str().unwrap_or_default()
    ));

    let msg = "committing node annotations";

    let mut node_anno_tab_csv = postgresql_import_reader(node_anno_tab_path.as_path())?;

    for result in node_anno_tab_csv.records() {
        let line = result?;

        let col_id = line.get(0).ok_or("Missing column")?;
        let node_id: NodeID = col_id.parse()?;
        let node_name = id_to_node_name.get(&node_id)?.ok_or("Missing node name")?;
        let col_ns = get_field_str(&line, 1).ok_or("Missing column")?;
        let col_name = get_field_str(&line, 2).ok_or("Missing column")?;
        let col_val = get_field_str(&line, 3).ok_or("Missing column")?;
        // we have to make some sanity checks
        if col_ns != "annis" || col_name != "tok" {
            let anno_val: String = if col_val == "NULL" {
                // use an "invalid" string so it can't be found by its value, but only by its annotation name
                std::char::MAX.to_string()
            } else {
                col_val
            };

            let col_ns = if col_ns == "NULL" {
                String::default()
            } else {
                col_ns
            };

            updater.add_event(
                UpdateEvent::AddNodeLabel {
                    node_name: node_name.clone(),
                    anno_ns: col_ns,
                    anno_name: col_name,
                    anno_value: anno_val.clone(),
                },
                Some(msg),
                progress_callback,
            )?;

            // add all missing span values from the annotation, but don't add NULL values
            if let Some(seg) = missing_seg_span.get(&node_id) {
                if seg == &get_field_str(&line, 2).ok_or("Missing column")?
                    && get_field_str(&line, 3).ok_or("Missing column")? != "NULL"
                {
                    updater.add_event(
                        UpdateEvent::AddNodeLabel {
                            node_name: node_name.clone(),
                            anno_ns: ANNIS_NS.to_owned(),
                            anno_name: TOK.to_owned(),
                            anno_value: anno_val,
                        },
                        Some(msg),
                        progress_callback,
                    )?;
                }
            }
        }
    }

    updater.commit(Some(msg), progress_callback)?;

    Ok(())
}

fn load_component_tab<F>(
    path: &PathBuf,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<BTreeMap<u32, Component>>
where
    F: Fn(&str) -> (),
{
    let mut component_tab_path = PathBuf::from(path);
    component_tab_path.push(if is_annis_33 {
        "component.annis"
    } else {
        "component.tab"
    });

    progress_callback(&format!(
        "loading {}",
        component_tab_path.to_str().unwrap_or_default()
    ));

    let mut component_by_id: BTreeMap<u32, Component> = BTreeMap::new();

    let mut component_tab_csv = postgresql_import_reader(component_tab_path.as_path())?;
    for result in component_tab_csv.records() {
        let line = result?;

        let cid: u32 = line.get(0).ok_or("Missing column")?.parse()?;
        let col_type = get_field_str(&line, 1).ok_or("Missing column")?;
        if col_type != "NULL" {
            let layer = get_field_str(&line, 2).ok_or("Missing column")?;
            let name = get_field_str(&line, 3).ok_or("Missing column")?;
            let name = if name == "NULL" {
                String::from("")
            } else {
                name
            };
            let ctype = component_type_from_short_name(&col_type)?;
            component_by_id.insert(cid, Component { ctype, layer, name });
        }
    }
    Ok(component_by_id)
}

fn load_nodes<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    texts: &mut HashMap<TextKey, Text>,
    corpus_table: &ParsedCorpusTable,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<LoadNodeResult>
where
    F: Fn(&str) -> (),
{
    let node_tab_parse_result = load_node_tab(
        path,
        updater,
        texts,
        corpus_table,
        is_annis_33,
        progress_callback,
    )?;

    for order_component in updater
        .g
        .get_all_components(Some(ComponentType::Ordering), None)
    {
        updater.g.calculate_component_statistics(&order_component)?;
        updater.g.optimize_impl(&order_component)?;
    }

    load_node_anno_tab(
        path,
        updater,
        &node_tab_parse_result.missing_seg_span,
        &node_tab_parse_result.id_to_node_name,
        is_annis_33,
        progress_callback,
    )?;

    Ok(LoadNodeResult {
        nodes_by_text: node_tab_parse_result.nodes_by_text,
        id_to_node_name: node_tab_parse_result.id_to_node_name,
        textpos_table: node_tab_parse_result.textpos_table,
    })
}

fn load_rank_tab<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    component_by_id: &BTreeMap<u32, Component>,
    id_to_node_name: &DiskMap<NodeID, String>,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<LoadRankResult>
where
    F: Fn(&str) -> (),
{
    let mut rank_tab_path = PathBuf::from(path);
    rank_tab_path.push(if is_annis_33 {
        "rank.annis"
    } else {
        "rank.tab"
    });

    progress_callback(&format!(
        "loading {}",
        rank_tab_path.to_str().unwrap_or_default()
    ));

    let mut load_rank_result = LoadRankResult {
        components_by_pre: DiskMap::default(),
        edges_by_pre: DiskMap::default(),
        text_coverage_edges: DiskMap::default(),
    };

    let mut rank_tab_csv = postgresql_import_reader(rank_tab_path.as_path())?;

    let pos_node_ref = if is_annis_33 { 3 } else { 2 };
    let pos_component_ref = if is_annis_33 { 4 } else { 3 };
    let pos_parent = if is_annis_33 { 5 } else { 4 };

    // first run: collect all pre-order values for a node
    let mut pre_to_node_id: BTreeMap<u32, NodeID> = BTreeMap::new();
    for result in rank_tab_csv.records() {
        let line = result?;
        let pre: u32 = line.get(0).ok_or("Missing column")?.parse()?;
        let node_id: NodeID = line.get(pos_node_ref).ok_or("Missing column")?.parse()?;
        pre_to_node_id.insert(pre, node_id);
    }

    // second run: get the actual edges
    let mut rank_tab_csv = postgresql_import_reader(rank_tab_path.as_path())?;

    let msg = "committing edges";

    for result in rank_tab_csv.records() {
        let line = result?;

        let parent_as_str = line.get(pos_parent).ok_or("Missing column")?;
        if parent_as_str != "NULL" {
            let parent: u32 = parent_as_str.parse()?;
            if let Some(source) = pre_to_node_id.get(&parent) {
                // find the responsible edge database by the component ID
                let component_ref: u32 = line
                    .get(pos_component_ref)
                    .ok_or("Missing column")?
                    .parse()?;
                if let Some(c) = component_by_id.get(&component_ref) {
                    let target: NodeID = line.get(pos_node_ref).ok_or("Missing column")?.parse()?;

                    updater.add_event(
                        UpdateEvent::AddEdge {
                            source_node: id_to_node_name
                                .get(&source)?
                                .ok_or("Missing node name")?
                                .to_owned(),
                            target_node: id_to_node_name
                                .get(&target)?
                                .ok_or("Missing node name")?
                                .to_owned(),
                            layer: c.layer.clone(),
                            component_type: c.ctype.to_string(),
                            component_name: c.name.clone(),
                        },
                        Some(msg),
                        progress_callback,
                    )?;

                    let pre: u32 = line.get(0).ok_or("Missing column")?.parse()?;

                    let e = Edge {
                        source: *source,
                        target,
                    };

                    if c.ctype == ComponentType::Coverage {
                        load_rank_result
                            .text_coverage_edges
                            .insert(e.clone(), true)?;
                    }
                    load_rank_result.components_by_pre.insert(pre, c.clone())?;
                    load_rank_result.edges_by_pre.insert(pre, e)?;
                }
            }
        }
    }

    info!(
        "creating index for content of {}",
        &rank_tab_path.to_string_lossy()
    );
    load_rank_result.components_by_pre.compact_and_flush()?;
    load_rank_result.edges_by_pre.compact_and_flush()?;
    load_rank_result.text_coverage_edges.compact_and_flush()?;

    updater.commit(Some(msg), progress_callback)?;

    Ok(load_rank_result)
}

fn load_edge_annotation<F>(
    path: &PathBuf,
    updater: &mut ChunkUpdater,
    rank_result: &LoadRankResult,
    id_to_node_name: &DiskMap<NodeID, String>,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    let mut edge_anno_tab_path = PathBuf::from(path);
    edge_anno_tab_path.push(if is_annis_33 {
        "edge_annotation.annis"
    } else {
        "edge_annotation.tab"
    });

    progress_callback(&format!(
        "loading {}",
        edge_anno_tab_path.to_str().unwrap_or_default()
    ));

    let msg = "committing edge annotations";

    let mut edge_anno_tab_csv = postgresql_import_reader(edge_anno_tab_path.as_path())?;

    for result in edge_anno_tab_csv.records() {
        let line = result?;

        let pre: u32 = line.get(0).ok_or("Missing column")?.parse()?;
        if let Some(c) = rank_result.components_by_pre.get(&pre)? {
            if let Some(e) = rank_result.edges_by_pre.get(&pre)? {
                let ns = get_field_str(&line, 1).ok_or("Missing column")?;
                let ns = if ns == "NULL" { String::default() } else { ns };
                let name = get_field_str(&line, 2).ok_or("Missing column")?;
                let val = get_field_str(&line, 3).ok_or("Missing column")?;

                updater.add_event(
                    UpdateEvent::AddEdgeLabel {
                        source_node: id_to_node_name
                            .get(&e.source)?
                            .ok_or("Missing node name")?
                            .to_owned(),
                        target_node: id_to_node_name
                            .get(&e.target)?
                            .ok_or("Missing node name")?
                            .to_owned(),
                        layer: c.layer.clone(),
                        component_type: c.ctype.to_string(),
                        component_name: c.name.clone(),
                        anno_ns: ns,
                        anno_name: name,
                        anno_value: val,
                    },
                    Some(msg),
                    progress_callback,
                )?;
            }
        }
    }

    updater.commit(Some(msg), progress_callback)?;

    Ok(())
}

fn load_corpus_annotation<F>(
    path: &PathBuf,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<BTreeMap<(u32, AnnoKey), String>>
where
    F: Fn(&str) -> (),
{
    let mut corpus_id_to_anno = BTreeMap::new();

    let mut corpus_anno_tab_path = PathBuf::from(path);
    corpus_anno_tab_path.push(if is_annis_33 {
        "corpus_annotation.annis"
    } else {
        "corpus_annotation.tab"
    });

    progress_callback(&format!(
        "loading {}",
        corpus_anno_tab_path.to_str().unwrap_or_default()
    ));

    let mut corpus_anno_tab_csv = postgresql_import_reader(corpus_anno_tab_path.as_path())?;

    for result in corpus_anno_tab_csv.records() {
        let line = result?;

        let id = line.get(0).ok_or("Missing column")?.parse()?;
        let ns = get_field_str(&line, 1).ok_or("Missing column")?;
        let ns = if ns == "NULL" { String::default() } else { ns };
        let name = get_field_str(&line, 2).ok_or("Missing column")?;
        let val = get_field_str(&line, 3).ok_or("Missing column")?;

        let anno_key = AnnoKey { ns, name };

        corpus_id_to_anno.insert((id, anno_key), val);
    }

    Ok(corpus_id_to_anno)
}

fn get_parent_path(cid: u32, corpus_table: &ParsedCorpusTable) -> Result<String> {
    let corpus = corpus_table
        .corpus_by_id
        .get(&cid)
        .ok_or_else(|| format!("Corpus with ID {} not found", cid))?;
    let pre = corpus.pre;
    let post = corpus.post;

    let parent_corpus_path: Vec<String> = corpus_table
        .corpus_by_preorder
        .range(0..pre)
        .filter_map(|(_, cid)| corpus_table.corpus_by_id.get(cid))
        .filter(|parent_corpus| post < parent_corpus.post)
        .map(|parent_corpus| {
            utf8_percent_encode(parent_corpus.name.as_ref(), SALT_URI_ENCODE_SET).to_string()
        })
        .collect();
    Ok(parent_corpus_path.join("/"))
}

fn get_corpus_path(cid: u32, corpus_table: &ParsedCorpusTable) -> Result<String> {
    let parent_path = get_parent_path(cid, corpus_table)?;
    let corpus = corpus_table
        .corpus_by_id
        .get(&cid)
        .ok_or_else(|| format!("Corpus with ID {} not found", cid))?;
    let corpus_name = utf8_percent_encode(&corpus.name, SALT_URI_ENCODE_SET).to_string();
    Ok(format!("{}/{}", parent_path, &corpus_name))
}

fn add_subcorpora<F>(
    updater: &mut ChunkUpdater,
    corpus_table: &ParsedCorpusTable,
    node_node_result: &LoadNodeResult,
    texts: &HashMap<TextKey, Text>,
    corpus_id_to_annos: &BTreeMap<(u32, AnnoKey), String>,
    is_annis_33: bool,
    progress_callback: &F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    let msg = "committing corpus structure";
    // add the toplevel corpus as node
    {
        updater.add_event(
            UpdateEvent::AddNode {
                node_name: corpus_table.toplevel_corpus_name.to_owned(),
                node_type: "corpus".to_owned(),
            },
            Some(msg),
            progress_callback,
        )?;

        // save the relANNIS version as meta data attribute on the toplevel corpus
        updater.add_event(
            UpdateEvent::AddNodeLabel {
                node_name: corpus_table.toplevel_corpus_name.to_owned(),
                anno_ns: ANNIS_NS.to_owned(),
                anno_name: "relannis-version".to_owned(),
                anno_value: if is_annis_33 {
                    "3.3".to_owned()
                } else {
                    "3.2".to_owned()
                },
            },
            Some(msg),
            progress_callback,
        )?;

        // add all metadata for the top-level corpus node
        if let Some(cid) = corpus_table.corpus_by_preorder.get(&0) {
            let start_key = (
                *cid,
                AnnoKey {
                    ns: "".to_string(),
                    name: "".to_string(),
                },
            );
            for ((entry_cid, anno_key), val) in corpus_id_to_annos.range(start_key..) {
                if entry_cid == cid {
                    updater.add_event(
                        UpdateEvent::AddNodeLabel {
                            node_name: corpus_table.toplevel_corpus_name.to_owned(),
                            anno_ns: anno_key.ns.clone(),
                            anno_name: anno_key.name.clone(),
                            anno_value: val.clone(),
                        },
                        Some(msg),
                        progress_callback,
                    )?;
                } else {
                    break;
                }
            }
        }
    }

    // add all subcorpora/documents (start with the smallest pre-order)
    for (pre, corpus_id) in corpus_table.corpus_by_preorder.iter() {
        if *pre != 0 {
            let corpus = corpus_table
                .corpus_by_id
                .get(corpus_id)
                .ok_or_else(|| format!("Can't get name for corpus with ID {}", corpus_id))?;

            let corpus_name = &corpus.name;

            let subcorpus_full_name = get_corpus_path(*corpus_id, corpus_table)?;

            // add a basic node labels for the new (sub-) corpus/document
            updater.add_event(
                UpdateEvent::AddNode {
                    node_name: subcorpus_full_name.clone(),
                    node_type: "corpus".to_owned(),
                },
                Some(msg),
                progress_callback,
            )?;
            updater.add_event(
                UpdateEvent::AddNodeLabel {
                    node_name: subcorpus_full_name.clone(),
                    anno_ns: ANNIS_NS.to_owned(),
                    anno_name: "doc".to_owned(),
                    anno_value: corpus_name.to_owned(),
                },
                Some(msg),
                progress_callback,
            )?;

            // add all metadata for the document node
            let start_key = (
                *corpus_id,
                AnnoKey {
                    ns: "".to_string(),
                    name: "".to_string(),
                },
            );
            for ((entry_cid, anno_key), val) in corpus_id_to_annos.range(start_key..) {
                if entry_cid == corpus_id {
                    updater.add_event(
                        UpdateEvent::AddNodeLabel {
                            node_name: subcorpus_full_name.clone(),
                            anno_ns: anno_key.ns.clone(),
                            anno_name: anno_key.name.clone(),
                            anno_value: val.clone(),
                        },
                        Some(msg),
                        progress_callback,
                    )?;
                } else {
                    break;
                }
            }
            // add an edge from the document (or sub-corpus) to the top-level corpus
            updater.add_event(
                UpdateEvent::AddEdge {
                    source_node: subcorpus_full_name.clone(),
                    target_node: corpus_table.toplevel_corpus_name.to_owned(),
                    layer: ANNIS_NS.to_owned(),
                    component_type: ComponentType::PartOf.to_string(),
                    component_name: String::default(),
                },
                Some(msg),
                progress_callback,
            )?;
        } // end if not toplevel corpus
    } // end for each document/sub-corpus

    // add a node for each text and the connection between all sub-nodes of the text
    for (text_key, text) in texts {
        // add text node (including its name)
        if let Some(corpus_ref) = text_key.corpus_ref {
            let text_name = utf8_percent_encode(&text.name, SALT_URI_ENCODE_SET).to_string();
            let subcorpus_full_name = get_corpus_path(corpus_ref, corpus_table)?;
            let text_full_name = format!("{}#{}", &subcorpus_full_name, &text_name);

            updater.add_event(
                UpdateEvent::AddNode {
                    node_name: text_full_name.clone(),
                    node_type: "datasource".to_owned(),
                },
                Some(msg),
                progress_callback,
            )?;

            // add an edge from the text to the document
            updater.add_event(
                UpdateEvent::AddEdge {
                    source_node: text_full_name.clone(),
                    target_node: subcorpus_full_name,
                    layer: ANNIS_NS.to_owned(),
                    component_type: ComponentType::PartOf.to_string(),
                    component_name: String::default(),
                },
                Some(msg),
                progress_callback,
            )?;

            // find all nodes belonging to this text and add a relation
            let min_key = NodeByTextEntry {
                corpus_ref,
                text_id: text_key.id,
                node_id: NodeID::min_value(),
            };
            let max_key = NodeByTextEntry {
                corpus_ref,
                text_id: text_key.id,
                node_id: NodeID::max_value(),
            };
            for (text_entry, _) in node_node_result.nodes_by_text.range(min_key..=max_key)? {
                let n = text_entry.node_id;
                updater.add_event(
                    UpdateEvent::AddEdge {
                        source_node: node_node_result
                            .id_to_node_name
                            .get(&n)?
                            .ok_or("Missing node name")?
                            .clone(),
                        target_node: text_full_name.clone(),
                        layer: ANNIS_NS.to_owned(),
                        component_type: ComponentType::PartOf.to_string(),
                        component_name: String::default(),
                    },
                    Some(msg),
                    progress_callback,
                )?;
            }
        }
    } // end for each text

    updater.commit(Some(msg), progress_callback)?;

    Ok(())
}

fn component_type_from_short_name(short_type: &str) -> Result<ComponentType> {
    match short_type {
        "c" => Ok(ComponentType::Coverage),
        "d" => Ok(ComponentType::Dominance),
        "p" => Ok(ComponentType::Pointing),
        "o" => Ok(ComponentType::Ordering),
        _ => Err(format!("Invalid component type short name '{}'", short_type).into()),
    }
}
