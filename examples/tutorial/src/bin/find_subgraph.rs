use graphannis::corpusstorage::{QueryLanguage, ResultOrder};
use graphannis::graph::AnnotationStorage;
use graphannis::util;
use graphannis::CorpusStorage;
use std::path::PathBuf;

fn main() {
    let cs = CorpusStorage::with_auto_cache_size(&PathBuf::from("data"), true).unwrap();
    let matches = cs
        .find(
            &["tutorial"],
            "tok . tok",
            QueryLanguage::AQL,
            0,
            Some(100),
            ResultOrder::Normal,
        )
        .unwrap();
    for m in matches {
        println!("{}", m);
        // convert the match string to a list of node IDs
        let node_names = util::node_names_from_match(&m);
        let g = cs.subgraph("tutorial", node_names, 2, 2, None).unwrap();
        // find all nodes of type "node" (regular annotation nodes)
        let node_search =
            g.get_node_annos()
                .exact_anno_search(Some("annis"), "node_type", Some("node").into());
        println!("Number of nodes in subgraph: {}", node_search.count());
    }
}
