#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    /// Expiration date as unix timestamp in seconds since epoch and UTC
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(
        default,
        rename = "https://corpus-tools.org/annis/groups",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub groups: Vec<String>,
    #[serde(
        default,
        rename = "https://corpus-tools.org/annis/roles",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub roles: Vec<String>,
}
