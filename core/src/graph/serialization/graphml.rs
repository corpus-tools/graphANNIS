use crate::{
    annostorage::ValueSearch,
    graph::{update::GraphUpdate, Graph, ANNIS_NS, NODE_TYPE},
    types::{AnnoKey, Annotation, Component, ComponentType, Edge},
    util::{join_qname, split_qname},
};
use anyhow::Result;
use quick_xml::{
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
    Reader, Writer,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufReader, BufWriter, Write},
};

fn write_annotation_keys<CT: ComponentType, W: std::io::Write>(
    graph: &Graph<CT>,
    writer: &mut Writer<W>,
) -> Result<BTreeMap<AnnoKey, String>> {
    // Create the special annis::component key with ID "c"
    {
        let qname = join_qname(ANNIS_NS, "component");

        let mut key_start = BytesStart::borrowed_name("key".as_bytes());
        key_start.push_attribute(("id", "c"));
        key_start.push_attribute(("for", "edge"));
        key_start.push_attribute(("attr.name", qname.as_str()));
        key_start.push_attribute(("attr.type", "string"));

        writer.write_event(Event::Empty(key_start))?;
    }

    let mut key_id_mapping = BTreeMap::new();
    let mut id_counter = 0;

    // Create node annotation keys
    for key in graph.get_node_annos().annotation_keys() {
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

    // Create edge annotation keys for all components, but skip auto-generated ones
    let autogenerated_components: BTreeSet<Component<CT>> =
        ComponentType::update_graph_index_components(graph)
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

        let mut id = m.node.to_string();
        id.insert(0, 'n');

        node_start.push_attribute(("id", id.as_str()));
        let node_annotations = graph.get_node_annos().get_annotations_for_item(&m.node);
        if node_annotations.is_empty() {
            // Write an empty XML element without child nodes
            writer.write_event(Event::Empty(node_start))?;
        } else {
            writer.write_event(Event::Start(node_start))?;
            // Write all annotations of the node as "data" element
            for anno in node_annotations {
                write_data(anno, writer, key_id_mapping)?;
            }
            writer.write_event(Event::End(BytesEnd::borrowed(b"node")))?;
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
        if let Some(gs) = graph.get_graphstorage(&c) {
            for source in gs.source_nodes() {
                let mut source_id = source.to_string();
                source_id.insert(0, 'n');

                for target in gs.get_outgoing_edges(source) {
                    let mut target_id = target.to_string();
                    target_id.insert(0, 'n');

                    let edge = Edge { source, target };

                    let mut edge_id = edge_counter.to_string();
                    edge_counter += 1;
                    edge_id.insert(0, 'e');

                    let mut edge_start = BytesStart::borrowed_name(b"edge");
                    edge_start.push_attribute(("id", edge_id.as_str()));
                    edge_start.push_attribute(("source", source_id.as_str()));
                    edge_start.push_attribute(("target", target_id.as_str()));

                    writer.write_event(Event::Start(edge_start))?;

                    // Always write the special "annis::component" annotation which contains the component the edge is part of
                    let mut component_data_start = BytesStart::borrowed_name(b"data");
                    component_data_start.push_attribute(("key", "c"));
                    writer.write_event(Event::Start(component_data_start))?;
                    // Add the annotation value as internal text node
                    writer
                        .write_event(Event::Text(BytesText::from_plain(c.to_string().as_ref())))?;
                    writer.write_event(Event::End(BytesEnd::borrowed(b"data")))?;

                    // Write all annotations of the edge as "data" element
                    for anno in gs.get_anno_storage().get_annotations_for_item(&edge) {
                        write_data(anno, writer, key_id_mapping)?;
                    }
                    writer.write_event(Event::End(BytesEnd::borrowed(b"edge")))?;
                }
            }
        }
    }
    Ok(())
}

pub fn export<CT: ComponentType, W: std::io::Write>(graph: &Graph<CT>, output: W) -> Result<()> {
    // Always buffer the output
    let output = BufWriter::new(output);
    let mut writer = Writer::new_with_indent(output, b' ', 4);

    // Add XML declaration
    let xml_decl = BytesDecl::new(b"1.0", Some(b"UTF-8"), None);
    writer.write_event(Event::Decl(xml_decl))?;

    // Always write the root element
    writer.write_event(Event::Start(BytesStart::borrowed_name(b"graphml")))?;

    // Define all valid annotation ns/name pairs
    let key_id_mapping = write_annotation_keys(graph, &mut writer)?;

    // We are writing a single graph
    let mut graph_start = BytesStart::borrowed_name("graph".as_bytes());
    graph_start.push_attribute(("edgedefault", "directed"));
    writer.write_event(Event::Start(graph_start))?;

    // Write out all nodes
    write_nodes(graph, &mut writer, &key_id_mapping)?;

    // Write out all edges
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
    let mut buf = Vec::new();

    let mut depth = 0;

    loop {
        match reader.read_event(&mut buf)? {
            Event::Start(ref e) => {
                depth += 1;
                if depth == 2 && e.name() == b"key" {
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
                depth -= 1;
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
    todo!("collect all nodes")
}

fn read_edges<R: std::io::BufRead>(
    input: &mut R,
    updates: &mut GraphUpdate,
    keys: &BTreeMap<String, AnnoKey>,
) -> Result<()> {
    todo!("collect all edges")
}

pub fn import<CT: ComponentType, R: std::io::Read>(
    input: R,
    disk_based: bool,
) -> Result<Graph<CT>> {
    // Always buffer the read operations
    let mut input = BufReader::new(input);

    // 1. pass: collect each keys
    let keys = read_keys(&mut input)?;

    let mut g = Graph::new(disk_based)?;
    let mut updates = GraphUpdate::default();

    // 2. pass: read in all nodes
    read_nodes(&mut input, &mut updates, &keys)?;

    // 3. pass: read in all edges
    read_edges(&mut input, &mut updates, &keys)?;

    // Apply all updates
    g.apply_update(&mut updates, |msg| {
        info!("{}", msg);
    })?;

    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        graph::{GraphUpdate, UpdateEvent},
        types::DefaultComponentType,
    };
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
        export(&g, &mut xml_data).unwrap();
        let expected = include_str!("graphml_example.xml");
        let actual = String::from_utf8(xml_data).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn import_graphml() {
        let input_xml = include_str!("graphml_example.xml");
        let g: Graph<DefaultComponentType> = import(input_xml.as_bytes(), false).unwrap();
        // Check that all nodes, edges and annotations have been created

        let _first_node_id = g.get_node_id_from_name("first_node").unwrap();
        let _second_node_id = g.get_node_id_from_name("second_node").unwrap();
    }
}
