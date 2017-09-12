use annis::NodeID;
use annis::util::c_api::*;
use super::*;
use libc;

#[repr(C)]
pub struct annis_ASNodePtr(AnnoStorage<NodeID>);
#[repr(C)]
pub struct annis_ASEdgePtr(AnnoStorage<Edge>);


/*
AnnoStorage<Node>
*/

#[no_mangle]
pub extern "C" fn annis_asnode_new() -> *mut annis_ASNodePtr {
    let s = AnnoStorage::<NodeID>::new();
    Box::into_raw(Box::new(annis_ASNodePtr(s)))
}

#[no_mangle]
pub extern "C" fn annis_asnode_free(ptr: *mut annis_ASNodePtr) {
    if ptr.is_null() {
        return;
    };
    // take ownership and destroy the pointer
    unsafe { Box::from_raw(ptr) };
}

#[no_mangle]
pub extern "C" fn annis_asnode_insert(ptr: *mut annis_ASNodePtr, item: NodeID, anno: Annotation) {
    cast_mut!(ptr).insert(item, anno);
}

#[no_mangle]
pub extern "C" fn annis_asnode_remove(
    ptr: *mut annis_ASNodePtr,
    item: NodeID,
    key: AnnoKey,
) -> annis_Option_StringID {
    let r = cast_mut!(ptr).remove(&item, &key);
    return annis_Option_StringID::from(r);
}

#[no_mangle]
pub extern "C" fn annis_asnode_len(ptr: *const annis_ASNodePtr) -> libc::size_t {
    cast_const!(ptr).len()
}

#[no_mangle]
pub extern "C" fn annis_asnode_get(
    ptr: *const annis_ASNodePtr,
    item: NodeID,
    key: AnnoKey,
) -> annis_Option_StringID {
    annis_Option_StringID::from_ref(cast_const!(ptr).get(&item, &key))
}

#[no_mangle]
pub extern "C" fn annis_asnode_get_all(
    ptr: *const annis_ASNodePtr,
    item: NodeID,
) -> annis_Vec_Annotation {
    let orig_vec = cast_const!(ptr).get_all(&item);
    let r = annis_Vec_Annotation::wrap(&orig_vec);
    // transfer ownership to calling code
    std::mem::forget(r.v);
    return r;
}

#[no_mangle]
pub extern "C" fn annis_asnode_guess_max_count(
    ptr: *const annis_ASNodePtr,
    ns: annis_Option_StringID,
    name: StringID,
    lower_val: *const libc::c_char,
    upper_val: *const libc::c_char,
) -> libc::size_t {
    let lower_val_str: &std::ffi::CStr = unsafe {
        assert!(!lower_val.is_null());

        std::ffi::CStr::from_ptr(lower_val)
    };

    let upper_val_str: &std::ffi::CStr = unsafe {
        assert!(!upper_val.is_null());

        std::ffi::CStr::from_ptr(upper_val)
    };

    cast_const!(ptr).guess_max_count(ns.to_option(), 
        name, 
        lower_val_str.to_str().unwrap(), 
        upper_val_str.to_str().unwrap()
    )
}

/*
AnnoStorage<Edge>
*/


#[no_mangle]
pub extern "C" fn annis_asedge_new() -> *mut annis_ASEdgePtr {
    let s = AnnoStorage::<Edge>::new();
    Box::into_raw(Box::new(annis_ASEdgePtr(s)))
}

#[no_mangle]
pub extern "C" fn annis_asedge_free(ptr: *mut annis_ASEdgePtr) {
    if ptr.is_null() {
        return;
    };
    // take ownership and destroy the pointer
    unsafe { Box::from_raw(ptr) };
}

#[no_mangle]
pub extern "C" fn annis_asedge_insert(ptr: *mut annis_ASEdgePtr, item: Edge, anno: Annotation) {
    cast_mut!(ptr).insert(item, anno);
}

#[no_mangle]
pub extern "C" fn annis_asedge_remove(
    ptr: *mut annis_ASEdgePtr,
    item: Edge,
    key: AnnoKey,
) -> annis_Option_StringID {
    let r = cast_mut!(ptr).remove(&item, &key);
    return annis_Option_StringID::from(r);
}

#[no_mangle]
pub extern "C" fn annis_asedge_len(ptr: *const annis_ASEdgePtr) -> libc::size_t {
    return cast_const!(ptr).len();
}

#[no_mangle]
pub extern "C" fn annis_asedge_get(
    ptr: *const annis_ASEdgePtr,
    item: Edge,
    key: AnnoKey,
) -> annis_Option_StringID {
    annis_Option_StringID::from_ref(cast_const!(ptr).get(&item, &key))
}

#[no_mangle]
pub extern "C" fn annis_asedge_get_all(
    ptr: *const annis_ASEdgePtr,
    item: Edge,
) -> annis_Vec_Annotation {
    let orig_vec = cast_const!(ptr).get_all(&item);
    let r = annis_Vec_Annotation::wrap(&orig_vec);
    // transfer ownership to calling code
    std::mem::forget(r.v);
    return r;
}

#[no_mangle]
pub extern "C" fn annis_asedge_guess_max_count(
    ptr: *const annis_ASEdgePtr,
    ns: annis_Option_StringID,
    name: StringID,
    lower_val: *const libc::c_char,
    upper_val: *const libc::c_char,
) -> libc::size_t {
    let lower_val_str: &std::ffi::CStr = unsafe {
        assert!(!lower_val.is_null());

        std::ffi::CStr::from_ptr(lower_val)
    };

    let upper_val_str: &std::ffi::CStr = unsafe {
        assert!(!upper_val.is_null());

        std::ffi::CStr::from_ptr(upper_val)
    };

    cast_const!(ptr).guess_max_count(ns.to_option(), 
        name, 
        lower_val_str.to_str().unwrap(), 
        upper_val_str.to_str().unwrap()
    )
}
