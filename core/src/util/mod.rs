pub mod disk_collections;
pub mod memory_estimation;

pub fn regex_full_match(pattern: &str) -> String {
    let mut full_match_pattern = String::new();
    full_match_pattern.push_str(r"\A(");
    full_match_pattern.push_str(pattern);
    full_match_pattern.push_str(r")\z");

    full_match_pattern
}
