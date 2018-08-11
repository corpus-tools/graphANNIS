use libc;
use std;
use api::update::*;
use api;

/// Create a new graph update instance
#[no_mangle]
pub extern "C" fn annis_graphupdate_new() -> *mut GraphUpdate {
    let gu = GraphUpdate::new();
    return Box::into_raw(Box::new(gu));
}


#[no_mangle]
pub extern "C" fn annis_graphupdate_add_node(
    ptr: *mut GraphUpdate,
    node_name: *const libc::c_char,
    node_type: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::AddNode {
        node_name: String::from(cstr!(node_name)),
        node_type: String::from(cstr!(node_type)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_delete_node(
    ptr: *mut GraphUpdate,
    node_name: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::DeleteNode {
        node_name: String::from(cstr!(node_name)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_add_node_label(
    ptr: *mut GraphUpdate,
    node_name: *const libc::c_char,
    anno_ns: *const libc::c_char,
    anno_name: *const libc::c_char,
    anno_value: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::AddNodeLabel {
        node_name: String::from(cstr!(node_name)),
        anno_ns: String::from(cstr!(anno_ns)),
        anno_name: String::from(cstr!(anno_name)),
        anno_value: String::from(cstr!(anno_value)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_delete_node_label(
    ptr: *mut GraphUpdate,
    node_name: *const libc::c_char,
    anno_ns: *const libc::c_char,
    anno_name: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::DeleteNodeLabel {
        node_name: String::from(cstr!(node_name)),
        anno_ns: String::from(cstr!(anno_ns)),
        anno_name: String::from(cstr!(anno_name)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_add_edge(
    ptr: *mut GraphUpdate,
    source_node: *const libc::c_char,
    target_node: *const libc::c_char,
    layer: *const libc::c_char,
    component_type: *const libc::c_char,
    component_name: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::AddEdge {
        source_node: String::from(cstr!(source_node)),
        target_node: String::from(cstr!(target_node)),
        layer: String::from(cstr!(layer)),
        component_type: String::from(cstr!(component_type)),
        component_name: String::from(cstr!(component_name)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_delete_edge(
    ptr: *mut GraphUpdate,
    source_node: *const libc::c_char,
    target_node: *const libc::c_char,
    layer: *const libc::c_char,
    component_type: *const libc::c_char,
    component_name: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::DeleteEdge {
        source_node: String::from(cstr!(source_node)),
        target_node: String::from(cstr!(target_node)),
        layer: String::from(cstr!(layer)),
        component_type: String::from(cstr!(component_type)),
        component_name: String::from(cstr!(component_name)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_add_edge_label(
    ptr: *mut GraphUpdate,
    source_node: *const libc::c_char,
    target_node: *const libc::c_char,
    layer: *const libc::c_char,
    component_type: *const libc::c_char,
    component_name: *const libc::c_char,
    anno_ns: *const libc::c_char,
    anno_name: *const libc::c_char,
    anno_value: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::AddEdgeLabel {
        source_node: String::from(cstr!(source_node)),
        target_node: String::from(cstr!(target_node)),
        layer: String::from(cstr!(layer)),
        component_type: String::from(cstr!(component_type)),
        component_name: String::from(cstr!(component_name)),
        anno_ns: String::from(cstr!(anno_ns)),
        anno_name: String::from(cstr!(anno_name)),
        anno_value: String::from(cstr!(anno_value)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_delete_edge_label(
    ptr: *mut GraphUpdate,
    source_node: *const libc::c_char,
    target_node: *const libc::c_char,
    layer: *const libc::c_char,
    component_type: *const libc::c_char,
    component_name: *const libc::c_char,
    anno_ns: *const libc::c_char,
    anno_name: *const libc::c_char,
) {
    let cs: &mut GraphUpdate = cast_mut!(ptr);
    cs.add_event(api::update::UpdateEvent::DeleteEdgeLabel {
        source_node: String::from(cstr!(source_node)),
        target_node: String::from(cstr!(target_node)),
        layer: String::from(cstr!(layer)),
        component_type: String::from(cstr!(component_type)),
        component_name: String::from(cstr!(component_name)),
        anno_ns: String::from(cstr!(anno_ns)),
        anno_name: String::from(cstr!(anno_name)),
    });
}

#[no_mangle]
pub extern "C" fn annis_graphupdate_size(
    ptr: *const GraphUpdate
) -> libc::size_t {
    let g : &GraphUpdate = cast_const!(ptr);
    return g.len();
}
