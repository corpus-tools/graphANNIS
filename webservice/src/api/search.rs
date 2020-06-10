use crate::AppState;
use actix_web::web;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct QueryParameters {
    q: String,
    query_language: String,
    corpora: String,
}

pub async fn count(info: web::Query<QueryParameters>, state: web::Data<AppState>) -> String {
    let corpora = vec![info.corpora.clone()];
    let count = state
        .cs
        .count(
            &corpora,
            &info.q,
            graphannis::corpusstorage::QueryLanguage::AQL,
        )
        .unwrap();
    format!("{}", count)
}
