
use annis::stringstorage::*;
use libc;
use std;

#[repr(C)]
pub struct annis_StringStoragePtr(StringStorage);

#[no_mangle]
pub extern "C" fn annis_stringstorage_new() -> *mut annis_StringStoragePtr {
    let s = StringStorage::new();
    Box::into_raw(Box::new(annis_StringStoragePtr(s)))
}

#[no_mangle]
pub extern "C" fn annis_stringstorage_free(target: *mut annis_StringStoragePtr) {
    // take ownership and destroy the pointer
    unsafe { Box::from_raw(target) };
}


#[repr(C)]
pub struct annis_OptionalString {
    pub valid: libc::c_int,
    pub value: *const libc::c_char,
    pub length: libc::size_t,
}

#[no_mangle]
pub extern "C" fn annis_stringstorage_str(target: *const annis_StringStoragePtr,
                                          id: libc::uint32_t)
                                          -> annis_OptionalString {

    let s = unsafe { &(*target).0 };
    let result = match s.str(id) {
        Some(v) => {
            annis_OptionalString {
                valid: 1,
                value: v.as_ptr() as *const libc::c_char,
                length: v.len(),
            }
        }
        None => {
            annis_OptionalString {
                valid: 0,
                value: std::ptr::null(),
                length: 0,
            }
        }
    };

    return result;
}

#[no_mangle]
pub extern "C" fn annis_stringstorage_add(target: *mut annis_StringStoragePtr,
                                          value: *const libc::c_char)
                                          -> libc::uint32_t {
    let mut s = unsafe { &mut(*target).0 };
    let wrapped_str = unsafe { std::ffi::CStr::from_ptr(value)};

    match std::str::from_utf8(wrapped_str.to_bytes()) {
        Ok(v) => s.add(v),
        Err(_) => 0
    }
}

#[no_mangle]
pub extern "C" fn annis_stringstorage_clear(target: *mut annis_StringStoragePtr) {
    let mut s = unsafe { &mut (*target).0 };
    s.clear();
}

#[no_mangle]
pub extern "C" fn annis_stringstorage_len(target: *const annis_StringStoragePtr)
                                          -> libc::size_t {
    let s = unsafe { & (*target).0 };
    return s.len();
}
