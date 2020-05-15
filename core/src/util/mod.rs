use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::borrow::Cow;

pub mod disk_collections;
pub mod memory_estimation;

const QNAME_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b':').add(b'%');

pub fn join_qname(ns: &str, name: &str) -> String {
    let mut result = String::with_capacity(ns.len() + name.len() + 2);
    if !ns.is_empty() {
        let encoded_anno_ns: Cow<str> = utf8_percent_encode(ns, QNAME_ENCODE_SET).into();
        result.push_str(&encoded_anno_ns);
        result.push_str("::");
    }
    let encoded_anno_name: Cow<str> = utf8_percent_encode(name, QNAME_ENCODE_SET).into();
    result.push_str(&encoded_anno_name);
    result
}

pub fn regex_full_match(pattern: &str) -> String {
    let mut full_match_pattern = String::new();
    full_match_pattern.push_str(r"\A(");
    full_match_pattern.push_str(pattern);
    full_match_pattern.push_str(r")\z");

    full_match_pattern
}
