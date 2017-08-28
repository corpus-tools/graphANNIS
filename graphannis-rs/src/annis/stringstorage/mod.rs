use annis::{StringID};
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use regex::Regex;
use std;
use bincode;

#[derive(Serialize, Deserialize, Debug)]
pub struct StringStorage {
    by_id: HashMap<StringID, String>,
    by_value: BTreeMap<String, StringID>,
}


impl StringStorage {
    pub fn new() -> StringStorage {
        StringStorage {
            by_id: HashMap::new(),
            by_value: BTreeMap::new(),
        }
    }

    pub fn str(&self, id: StringID) -> Option<&String> {
        return self.by_id.get(&id);
    }

    pub fn add(&mut self, val: &str) -> StringID {
        {
            let existing = self.by_value.get(val);
            if existing.is_some() {
                return *(existing.unwrap());
            }
        }
        // non-existing: add a new value
        let mut id = self.by_id.len() as StringID + 1; // since 0 is taken as ANY value begin with 1
        while self.by_id.get(&id).is_some() {
            id = id + 1;
        }
        // add the new entry to both maps
        self.by_id.insert(id, String::from(val));
        self.by_value.insert(String::from(val), id);

        return id;
    }

    pub fn find_id(&self, val: &str) -> Option<&StringID> {
        return self.by_value.get(&String::from(val));
    }

    pub fn find_regex(&self, val: &str) -> BTreeSet<&StringID> {
        let mut result = BTreeSet::new();

        // we always want to match the complete string
        let mut full_match_pattern = String::new();
        full_match_pattern.push_str(r"\A");
        full_match_pattern.push_str(val);
        full_match_pattern.push_str(r"\z");

        let compiled_result = Regex::new(&full_match_pattern);
        if compiled_result.is_ok() {
            let re = compiled_result.unwrap();

            // check all values
            // TODO: get a valid prefix somehow and check only a range of strings, not all
            for (s, id) in &self.by_value {
                if re.is_match(s) {
                    result.insert(id);
                }
            }
        }

        return result;
    }

    pub fn avg_length(&self) -> f64 {
        let mut sum: usize = 0;
        for (s, _) in &self.by_value {
            sum += s.len();
        }
        return (sum as f64) / (self.by_value.len() as f64);
    }

    pub fn len(&self) -> usize {
        return self.by_id.len();
    }

    pub fn clear(&mut self) {
        self.by_id.clear();
        self.by_value.clear();
    }

    #[allow(unused_must_use)]
    pub fn save_to_file(&self, path: &str) {

        let f = std::fs::File::create(path).unwrap();

        let mut buf_writer = std::io::BufWriter::new(f);

        bincode::serialize_into(&mut buf_writer, self, bincode::Infinite);
    }

    pub fn load_from_file(&mut self, path: &str) {

        // always remove all entries first, so even if there is an error the string storage is empty
        self.clear();

        let f = std::fs::File::open(path);
        if f.is_ok() {
            let mut buf_reader = std::io::BufReader::new(f.unwrap());

            let loaded: Result<StringStorage, _> =
                bincode::deserialize_from(&mut buf_reader, bincode::Infinite);
            if loaded.is_ok() {
                *self = loaded.unwrap();
            }
        }
    }

    pub fn estimate_memory_size(&self) -> usize {

        return ::annis::util::memory_estimation::hash_map_size(&self.by_id) +
            ::annis::util::memory_estimation::btree_map_size(&self.by_value);
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    extern crate tempdir;

    #[test]
    fn insert_and_get() {
        let mut s = StringStorage::new();
        let id1 = s.add("abc");
        let id2 = s.add("def");
        let id3 = s.add("def");

        assert_eq!(2, s.len());

        assert_eq!(id2, id3);

        {
            let x = s.str(id1);
            match x {
                Some(v) => assert_eq!("abc", v),
                None => panic!("Did not find string"),
            }
        }
        s.clear();
        assert_eq!(0, s.len());
    }

    #[test]
    fn insert_clear_insert_get() {
        let mut s = StringStorage::new();

        s.add("abc");
        assert_eq!(1, s.len());
        s.clear();
        assert_eq!(0, s.len());
        s.add("abc");
        assert_eq!(1, s.len());    
    }

    #[test]
    fn serialization() {
        let mut s = StringStorage::new();
        s.add("abc");
        s.add("def");

        if let Ok(tmp) = tempdir::TempDir::new("annis_test") {
            let file_path = tmp.path().join("out.storage");
            let file_path_str = file_path.to_str().unwrap();
            s.save_to_file(&file_path_str);

            s.clear();

            s.load_from_file(&file_path_str);
            assert_eq!(2, s.len());
        }
    }
}

pub mod c_api {

    use libc;
    use std::ffi::CStr;
    use annis::util::c_api::*;
    use super::*;
    

    #[repr(C)]
    pub struct annis_StringStoragePtr(StringStorage);

    

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_new() -> *mut annis_StringStoragePtr {
        let s = StringStorage::new();
        Box::into_raw(Box::new(annis_StringStoragePtr(s)))
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_free(ptr: *mut annis_StringStoragePtr) {
        if ptr.is_null() {
            return;
        };
        // take ownership and destroy the pointer
        unsafe { Box::from_raw(ptr) };
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_str(
        ptr: *const annis_StringStoragePtr,
        id: libc::uint32_t,
    ) -> annis_Option_String {

        let s = unsafe {
            assert!(!ptr.is_null());
            &(*ptr).0
        };
        let result = match s.str(id) {
            Some(v) => annis_Option_String {
                valid: true,
                value: annis_String {s: v.as_ptr() as *const libc::c_char, length: v.len()} ,
            },
            None => annis_Option_String {
                valid: false,
                value: annis_String {s: std::ptr::null(), length: 0},
            },
        };

        return result;
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_find_id(
        ptr: *const annis_StringStoragePtr,
        value: *const libc::c_char,
    ) -> annis_Option_u32 {
        let s = unsafe {
            assert!(!ptr.is_null());
            &(*ptr).0
        };
        let c_value = unsafe {
            assert!(!value.is_null());
            CStr::from_ptr(value)
        };

        let result = match c_value.to_str() {
            Ok(v) => match s.find_id(v) {
                Some(x) => annis_Option_u32 {
                    valid: true,
                    value: *x,
                },
                None => annis_Option_u32 { valid: false, value: 0 },
            },
            Err(_) => annis_Option_u32 { valid: false, value: 0 },
        };

        return result;
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_add(
        ptr: *mut annis_StringStoragePtr,
        value: *const libc::c_char,
    ) -> libc::uint32_t {
        let s = unsafe {
            assert!(!ptr.is_null());
            &mut (*ptr).0
        };
        let c_value = unsafe {
            assert!(!value.is_null());
            CStr::from_ptr(value)
        };

        match c_value.to_str() {
            Ok(v) => s.add(v),
            Err(_) => 0,
        }
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_clear(ptr: *mut annis_StringStoragePtr) {
        let s = unsafe {
            assert!(!ptr.is_null());
            &mut (*ptr).0
        };
        s.clear();
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_len(ptr: *const annis_StringStoragePtr) -> libc::size_t {
        let s = unsafe {
            assert!(!ptr.is_null());
            &(*ptr).0
        };
        return s.len();
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_avg_length(
        ptr: *const annis_StringStoragePtr,
    ) -> libc::c_double {
        let s = unsafe {
            assert!(!ptr.is_null());
            &(*ptr).0
        };
        return s.avg_length();
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_save_to_file(
        ptr: *const annis_StringStoragePtr,
        path: *const libc::c_char,
    ) {
        let s = unsafe {
            assert!(!ptr.is_null());
            &(*ptr).0
        };
        let c_path = unsafe {
            assert!(!path.is_null());
            CStr::from_ptr(path)
        };
        let safe_path = c_path.to_str();
        if safe_path.is_ok() {
            s.save_to_file(safe_path.unwrap());
        }
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_load_from_file(
        ptr: *mut annis_StringStoragePtr,
        path: *const libc::c_char,
    ) {
        let s = unsafe {
            assert!(!ptr.is_null());
            &mut (*ptr).0
        };
        let c_path = unsafe {
            assert!(!path.is_null());
            CStr::from_ptr(path)
        };
        let safe_path = c_path.to_str();
        if safe_path.is_ok() {
            s.load_from_file(safe_path.unwrap());
        }
    }

    #[no_mangle]
    pub extern "C" fn annis_stringstorage_estimate_memory(
        ptr: *const annis_StringStoragePtr,
    ) -> libc::size_t {
        let s = unsafe {
            assert!(!ptr.is_null());
            &(*ptr).0
        };

        return s.estimate_memory_size();
    }
}
