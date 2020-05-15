use crate::{
    annostorage::ValueSearch,
    graph::{
        update::{GraphUpdate, UpdateEvent},
        Graph, ANNIS_NS, NODE_NAME, NODE_NAME_KEY, NODE_TYPE, NODE_TYPE_KEY,
    },
    types::{AnnoKey, Annotation, Component, ComponentType, Edge},
    util::{join_qname, split_qname},
};
use anyhow::Result;
use quick_xml::{
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
    Reader, Writer,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    str::FromStr,
};

fn write_annotation_keys<CT: ComponentType, W: std::io::Write>(
    graph: &Graph<CT>,
    writer: &mut Writer<W>,
) -> Result<BTreeMap<AnnoKey, String>> {
    let mut key_id_mapping = BTreeMap::new();
    let mut id_counter = 0;

    // Create node annotation keys
    for key in graph.get_node_annos().annotation_keys() {
        if key.ns != ANNIS_NS || key.name != NODE_NAME {
            if !key_id_mapping.contains_key(&key) {
                let new_id = format!("k{}", id_counter);
                id_counter += 1;

                let qname = join_qname(&key.ns, &key.name);

                let mut key_start = BytesStart::borrowed_name("key".as_bytes());
                key_start.push_attribute(("id", new_id.as_str()));
                key_start.push_attribute(("for", "node"));
                key_start.push_attribute(("attr.name", qname.as_str()));
                key_start.push_attribute(("attr.type", "string"));

                writer.write_event(Event::Empty(key_start))?;

                key_id_mapping.insert(key, new_id);
            }
        }
    }

    // Create edge annotation keys for all components, but skip auto-generated ones
    let autogenerated_components: BTreeSet<Component<CT>> =
        CT::update_graph_index_components(graph)
            .into_iter()
            .collect();
    for c in graph.get_all_components(None, None) {
        if !autogenerated_components.contains(&c) {
            if let Some(gs) = graph.get_graphstorage(&c) {
                for key in gs.get_anno_storage().annotation_keys() {
                    if !key_id_mapping.contains_key(&key) {
                        let new_id = format!("k{}", id_counter);
                        id_counter += 1;

                        let qname = join_qname(&key.ns, &key.name);

                        let mut key_start = BytesStart::borrowed_name("key".as_bytes());
                        key_start.push_attribute(("id", new_id.as_str()));
                        key_start.push_attribute(("for", "node"));
                        key_start.push_attribute(("attr.name", qname.as_str()));
                        key_start.push_attribute(("attr.type", "string"));

                        writer.write_event(Event::Empty(key_start))?;

                        key_id_mapping.insert(key, new_id);
                    }
                }
            }
        }
    }

    Ok(key_id_mapping)
}

fn write_data<W: std::io::Write>(
    anno: Annotation,
    writer: &mut Writer<W>,
    key_id_mapping: &BTreeMap<AnnoKey, String>,
) -> Result<()> {
    let mut data_start = BytesStart::borrowed_name(b"data");

    let key_id = key_id_mapping.get(&anno.key).ok_or_else(|| {
        anyhow!(
            "Could not find annotation key ID for {:?} when mapping to GraphML",
            &anno.key
        )
    })?;

    data_start.push_attribute(("key", key_id.as_str()));
    writer.write_event(Event::Start(data_start))?;
    // Add the annotation value as internal text node
    writer.write_event(Event::Text(BytesText::from_plain(anno.val.as_bytes())))?;
    writer.write_event(Event::End(BytesEnd::borrowed(b"data")))?;

    Ok(())
}

fn write_nodes<CT: ComponentType, W: std::io::Write>(
    graph: &Graph<CT>,
    writer: &mut Writer<W>,
    key_id_mapping: &BTreeMap<AnnoKey, String>,
) -> Result<()> {
    for m in graph
        .get_node_annos()
        .exact_anno_search(Some(ANNIS_NS), NODE_TYPE, ValueSearch::Any)
    {
        let mut node_start = BytesStart::borrowed_name("node".as_bytes());

        if let Some(id) = graph
            .get_node_annos()
            .get_value_for_item(&m.node, &NODE_NAME_KEY)
        {
            node_start.push_attribute(("id", id.as_ref()));
            let node_annotations = graph.get_node_annos().get_annotations_for_item(&m.node);
            if node_annotations.is_empty() {
                // Write an empty XML element without child nodes
                writer.write_event(Event::Empty(node_start))?;
            } else {
                writer.write_event(Event::Start(node_start))?;
                // Write all annotations of the node as "data" element
                for anno in node_annotations {
                    if anno.key.ns != ANNIS_NS || anno.key.name != NODE_NAME {
                        write_data(anno, writer, key_id_mapping)?;
                    }
                }
                writer.write_event(Event::End(BytesEnd::borrowed(b"node")))?;
            }
        }
    }
    Ok(())
}

fn write_edges<CT: ComponentType, W: std::io::Write>(
    graph: &Graph<CT>,
    writer: &mut Writer<W>,
    key_id_mapping: &BTreeMap<AnnoKey, String>,
) -> Result<()> {
    let mut edge_counter = 0;
    for c in graph.get_all_components(None, None) {
        // Create edge annotation keys for all components, but skip auto-generated ones
        let autogenerated_components: BTreeSet<Component<CT>> =
            CT::update_graph_index_components(graph)
                .into_iter()
                .collect();
        if !autogenerated_components.contains(&c) {
            if let Some(gs) = graph.get_graphstorage(&c) {
                for source in gs.source_nodes() {
                    if let Some(source_id) = graph
                        .get_node_annos()
                        .get_value_for_item(&source, &NODE_NAME_KEY)
                    {
                        for target in gs.get_outgoing_edges(source) {
                            if let Some(target_id) = graph
                                .get_node_annos()
                                .get_value_for_item(&target, &NODE_NAME_KEY)
                            {
                                let edge = Edge { source, target };

                                let mut edge_id = edge_counter.to_string();
                                edge_counter += 1;
                                edge_id.insert(0, 'e');

                                let mut edge_start = BytesStart::borrowed_name(b"edge");
                                edge_start.push_attribute(("id", edge_id.as_str()));
                                edge_start.push_attribute(("source", source_id.as_ref()));
                                edge_start.push_attribute(("target", target_id.as_ref()));
                                // Use the "label" attribute as component type. This is consistent with how Neo4j interprets this non-standard attribute
                                edge_start.push_attribute(("label", c.to_string().as_ref()));

                                writer.write_event(Event::Start(edge_start))?;

                                // Write all annotations of the edge as "data" element
                                for anno in gs.get_anno_storage().get_annotations_for_item(&edge) {
                                    write_data(anno, writer, key_id_mapping)?;
                                }
                                writer.write_event(Event::End(BytesEnd::borrowed(b"edge")))?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn export<CT: ComponentType, W: std::io::Write, F>(
    graph: &Graph<CT>,
    output: W,
    progress_callback: F,
) -> Result<()>
where
    F: Fn(&str) -> (),
{
    // Always buffer the output
    let output = BufWriter::new(output);
    let mut writer = Writer::new_with_indent(output, b' ', 4);

    // Add XML declaration
    let xml_decl = BytesDecl::new(b"1.0", Some(b"UTF-8"), None);
    writer.write_event(Event::Decl(xml_decl))?;

    // Always write the root element
    writer.write_event(Event::Start(BytesStart::borrowed_name(b"graphml")))?;

    // Define all valid annotation ns/name pairs
    progress_callback("exporting all available annotation keys");
    let key_id_mapping = write_annotation_keys(graph, &mut writer)?;

    // We are writing a single graph
    let mut graph_start = BytesStart::borrowed_name("graph".as_bytes());
    graph_start.push_attribute(("edgedefault", "directed"));
    writer.write_event(Event::Start(graph_start))?;

    // Write out all nodes
    progress_callback("exporting nodes");
    write_nodes(graph, &mut writer, &key_id_mapping)?;

    // Write out all edges
    progress_callback("exporting edges");
    write_edges(graph, &mut writer, &key_id_mapping)?;

    writer.write_event(Event::End(BytesEnd::borrowed(b"graph")))?;
    writer.write_event(Event::End(BytesEnd::borrowed(b"graphml")))?;

    // Make sure to flush the buffered writer
    writer.into_inner().flush()?;

    Ok(())
}

fn read_keys<R: std::io::BufRead>(input: &mut R) -> Result<BTreeMap<String, AnnoKey>> {
    let mut result = BTreeMap::new();

    let mut reader = Reader::from_reader(input);
    reader.expand_empty_elements(true);
    let mut buf = Vec::new();

    let mut level = 0;

    loop {
        match reader.read_event(&mut buf)? {
            Event::Start(ref e) => {
                level += 1;
                if level == 2 && e.name() == b"key" {
                    // resolve the ID to the fully qualified annotation name
                    let mut id: Option<String> = None;
                    let mut anno_key: Option<AnnoKey> = None;

                    for att in e.attributes() {
                        let att = att?;

                        let att_value = String::from_utf8_lossy(&att.value);

                        match att.key {
                            b"id" => {
                                id = Some(att_value.to_string());
                            }
                            b"attr.name" => {
                                let (ns, name) = split_qname(att_value.as_ref());
                                anno_key = Some(AnnoKey {
                                    ns: ns.unwrap_or("").to_string(),
                                    name: name.to_string(),
                                });
                            }
                            _ => {}
                        }
                    }

                    if let (Some(id), Some(anno_key)) = (id, anno_key) {
                        result.insert(id.to_string(), anno_key);
                    }
                }
            }
            Event::End(_) => {
                level -= 1;
            }
            Event::Eof => {
                break;
            }
            _ => {}
        }
    }
    Ok(result)
}

fn read_nodes<R: std::io::BufRead>(
    input: &mut R,
    updates: &mut GraphUpdate,
    keys: &BTreeMap<String, AnnoKey>,
) -> Result<()> {
    let mut reader = Reader::from_reader(input);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();

    let mut level = 0;
    let mut in_graph = false;
    let mut current_node_id: Option<String> = None;
    let mut current_data_key: Option<String> = None;

    let mut data: HashMap<AnnoKey, String> = HashMap::new();

    loop {
        match reader.read_event(&mut buf)? {
            Event::Start(ref e) => {
                level += 1;

                match e.name() {
                    b"graph" => {
                        if level == 2 {
                            in_graph = true;
                        }
                    }
                    b"node" => {
                        if in_graph && level == 3 {
                            // Get the ID of this node
                            for att in e.attributes() {
                                let att = att?;
                                if att.key == b"id" {
                                    current_node_id =
                                        Some(String::from_utf8_lossy(&att.value).to_string());
                                }
                            }
                        }
                    }
                    b"data" => {
                        for att in e.attributes() {
                            let att = att?;
                            if att.key == b"key" {
                                current_data_key =
                                    Some(String::from_utf8_lossy(&att.value).to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(current_data_key) = &current_data_key {
                    if let Some(anno_key) = keys.get(current_data_key) {
                        // Copy all data attributes into our own map
                        data.insert(anno_key.clone(), t.unescape_and_decode(&reader)?);
                    }
                }
            }
            Event::End(ref e) => {
                match e.name() {
                    b"graph" => {
                        in_graph = false;
                    }
                    b"node" => {
                        if let Some(node_name) = current_node_id {
                            // Insert graph update for node
                            let node_type = data
                                .remove(&NODE_TYPE_KEY)
                                .unwrap_or_else(|| "node".to_string());
                            updates.add_event(UpdateEvent::AddNode {
                                node_name: node_name.clone(),
                                node_type,
                            })?;
                            // Add all remaining data entries as annotations
                            for (key, value) in data.drain() {
                                updates.add_event(UpdateEvent::AddNodeLabel {
                                    node_name: node_name.clone(),
                                    anno_ns: key.ns,
                                    anno_name: key.name,
                                    anno_value: value,
                                })?;
                            }
                        }

                        current_node_id = None;
                    }
                    b"data" => {
                        current_data_key = None;
                    }
                    _ => {}
                }

                level -= 1;
            }
            Event::Eof => {
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

fn read_edges<CT: ComponentType, R: std::io::BufRead>(
    input: &mut R,
    updates: &mut GraphUpdate,
    keys: &BTreeMap<String, AnnoKey>,
) -> Result<()> {
    let mut reader = Reader::from_reader(input);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();

    let mut level = 0;
    let mut in_graph = false;
    let mut current_source_id: Option<String> = None;
    let mut current_target_id: Option<String> = None;
    let mut current_data_key: Option<String> = None;
    let mut current_component: Option<String> = None;

    let mut data: HashMap<AnnoKey, String> = HashMap::new();

    loop {
        match reader.read_event(&mut buf)? {
            Event::Start(ref e) => {
                level += 1;

                match e.name() {
                    b"graph" => {
                        if level == 2 {
                            in_graph = true;
                        }
                    }
                    b"edge" => {
                        if in_graph && level == 3 {
                            // Get the source and target node IDs
                            for att in e.attributes() {
                                let att = att?;
                                if att.key == b"source" {
                                    current_source_id =
                                        Some(String::from_utf8_lossy(&att.value).to_string());
                                } else if att.key == b"target" {
                                    current_target_id =
                                        Some(String::from_utf8_lossy(&att.value).to_string());
                                } else if att.key == b"label" {
                                    current_component =
                                        Some(String::from_utf8_lossy(&att.value).to_string());
                                }
                            }
                        }
                    }
                    b"data" => {
                        for att in e.attributes() {
                            let att = att?;
                            if att.key == b"key" {
                                current_data_key =
                                    Some(String::from_utf8_lossy(&att.value).to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(current_data_key) = &current_data_key {
                    if let Some(anno_key) = keys.get(current_data_key) {
                        // Copy all data attributes into our own map
                        data.insert(anno_key.clone(), t.unescape_and_decode(&reader)?);
                    }
                }
            }
            Event::End(ref e) => {
                match e.name() {
                    b"graph" => {
                        in_graph = false;
                    }
                    b"edge" => {
                        if let (Some(source), Some(target), Some(component)) =
                            (current_source_id, current_target_id, current_component)
                        {
                            // Insert graph update for this edge
                            if let Ok(component) = Component::<CT>::from_str(&component) {
                                updates.add_event(UpdateEvent::AddEdge {
                                    source_node: source.clone(),
                                    target_node: target.clone(),
                                    layer: component.layer.clone(),
                                    component_type: component.get_type().to_string(),
                                    component_name: component.name.clone(),
                                })?;

                                // Add all remaining data entries as annotations
                                for (key, value) in data.drain() {
                                    updates.add_event(UpdateEvent::AddEdgeLabel {
                                        source_node: source.clone(),
                                        target_node: target.clone(),
                                        layer: component.layer.clone(),
                                        component_type: component.get_type().to_string(),
                                        component_name: component.name.clone(),
                                        anno_ns: key.ns,
                                        anno_name: key.name,
                                        anno_value: value,
                                    })?;
                                }
                            }
                        }

                        current_source_id = None;
                        current_target_id = None;
                        current_component = None;
                    }
                    b"data" => {
                        current_data_key = None;
                    }
                    _ => {}
                }

                level -= 1;
            }
            Event::Eof => {
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn import<CT: ComponentType, R: Read + Seek, F>(
    input: R,
    disk_based: bool,
    progress_callback: F,
) -> Result<Graph<CT>>
where
    F: Fn(&str) -> (),
{
    // Always buffer the read operations
    let mut input = BufReader::new(input);

    // 1. pass: collect each keys
    progress_callback("collecting all importable annotation keys");
    let keys = read_keys(&mut input)?;

    let mut g = Graph::new(disk_based)?;
    let mut updates = GraphUpdate::default();

    // 2. pass: read in all nodes
    input.seek(SeekFrom::Start(0))?;
    progress_callback("reading all nodes");
    read_nodes(&mut input, &mut updates, &keys)?;

    // 3. pass: read in all edges
    input.seek(SeekFrom::Start(0))?;
    progress_callback("reading all edges");
    read_edges::<CT, BufReader<R>>(&mut input, &mut updates, &keys)?;

    // Apply all updates
    g.apply_update(&mut updates, progress_callback)?;

    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{graph::GraphUpdate, types::DefaultComponentType};
    use std::borrow::Cow;
    #[test]
    fn export_graphml() {
        // Create a sample graph using the simple type
        let mut u = GraphUpdate::new();
        u.add_event(UpdateEvent::AddNode {
            node_name: "first_node".to_string(),
            node_type: "node".to_string(),
        })
        .unwrap();
        u.add_event(UpdateEvent::AddNode {
            node_name: "second_node".to_string(),
            node_type: "node".to_string(),
        })
        .unwrap();
        u.add_event(UpdateEvent::AddNodeLabel {
            node_name: "first_node".to_string(),
            anno_ns: "default_ns".to_string(),
            anno_name: "an_annotation".to_string(),
            anno_value: "something".to_string(),
        })
        .unwrap();

        u.add_event(UpdateEvent::AddEdge {
            source_node: "first_node".to_string(),
            target_node: "second_node".to_string(),
            component_type: "Edge".to_string(),
            layer: "some_ns".to_string(),
            component_name: "test_component".to_string(),
        })
        .unwrap();

        let mut g: Graph<DefaultComponentType> = Graph::new(false).unwrap();
        g.apply_update(&mut u, |_| {}).unwrap();

        // export to GraphML, read generated XML and compare it
        let mut xml_data: Vec<u8> = Vec::default();
        export(&g, &mut xml_data, |_| {}).unwrap();
        let expected = include_str!("graphml_example.graphml");
        let actual = String::from_utf8(xml_data).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn import_graphml() {
        let input_xml =
            std::io::Cursor::new(include_str!("graphml_example.graphml").as_bytes().to_owned());
        let g: Graph<DefaultComponentType> = import(input_xml, false, |_| {}).unwrap();

        // Check that all nodes, edges and annotations have been created
        let first_node_id = g.get_node_id_from_name("first_node").unwrap();
        let second_node_id = g.get_node_id_from_name("second_node").unwrap();

        let first_node_annos = g.get_node_annos().get_annotations_for_item(&first_node_id);
        assert_eq!(3, first_node_annos.len());
        assert_eq!(
            Some(Cow::Borrowed("something")),
            g.get_node_annos().get_value_for_item(
                &first_node_id,
                &AnnoKey {
                    ns: "default_ns".to_string(),
                    name: "an_annotation".to_string(),
                }
            )
        );

        assert_eq!(
            2,
            g.get_node_annos()
                .get_annotations_for_item(&second_node_id)
                .len()
        );

        let component = g.get_all_components(Some(DefaultComponentType::Edge), None);
        assert_eq!(1, component.len());
        assert_eq!("some_ns", component[0].layer);
        assert_eq!("test_component", component[0].name);

        let test_gs = g.get_graphstorage_as_ref(&component[0]).unwrap();
        assert_eq!(Some(1), test_gs.distance(first_node_id, second_node_id));
    }
}
