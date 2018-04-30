use {StringID};
use fxhash::{FxHashMap, FxHashSet};
use regex::Regex;
use std;
use bincode;

#[derive(Serialize, Deserialize, Clone, Debug, HeapSizeOf)]
pub struct StringStorage {
    by_id: FxHashMap<StringID, String>,
    by_value: FxHashMap<String, StringID>,
}


impl StringStorage {
    pub fn new() -> StringStorage {
        StringStorage {
            by_id: FxHashMap::default(),
            by_value: FxHashMap::default(),
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

    pub fn find_regex(&self, val: &str) -> FxHashSet<&StringID> {
        let mut result = FxHashSet::default();

        // we always want to match the complete string
        let full_match_pattern = ::util::regex_full_match(val);

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

    pub fn save_to_file(&self, path: &str) -> bool {

        let f = std::fs::File::create(path).unwrap();

        let mut buf_writer = std::io::BufWriter::new(f);

        bincode::serialize_into(&mut buf_writer, self).is_ok()
    }

    pub fn load_from_file(&mut self, path: &str) {

        // always remove all entries first, so even if there is an error the string storage is empty
        self.clear();

        let f = std::fs::File::open(path);
        if f.is_ok() {
            let mut buf_reader = std::io::BufReader::new(f.unwrap());

            let loaded: Result<StringStorage, _> =
                bincode::deserialize_from(&mut buf_reader);
            if loaded.is_ok() {
                *self = loaded.unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests;

