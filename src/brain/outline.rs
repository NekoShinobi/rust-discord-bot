use serde::{Deserialize, Serialize};

use crate::env::{OUTLINE_API_KEY, OUTLINE_COLLECTION_ID, OUTLINE_URL};
use crate::HTTP_CLIENT;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutlineDoc {
    pub id: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
struct SearchResultItem {
    document: OutlineDoc,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    data: Vec<SearchResultItem>,
}

#[derive(Debug, Deserialize)]
struct DocResponse {
    data: OutlineDoc,
}

fn auth_header() -> String {
    format!("Bearer {}", &*OUTLINE_API_KEY)
}

pub async fn search(query: &str, limit: u32) -> Result<Vec<OutlineDoc>, crate::Error> {
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/documents.search", &*OUTLINE_URL))
        .header("Authorization", auth_header())
        .json(&serde_json::json!({
            "query": query,
            "limit": limit,
            "collectionId": &*OUTLINE_COLLECTION_ID
        }))
        .send()
        .await?
        .json::<SearchResponse>()
        .await?;

    Ok(resp.data.into_iter().map(|r| r.document).collect())
}

pub async fn get_doc(id: &str) -> Result<OutlineDoc, crate::Error> {
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/documents.info", &*OUTLINE_URL))
        .header("Authorization", auth_header())
        .json(&serde_json::json!({ "id": id }))
        .send()
        .await?
        .json::<DocResponse>()
        .await?;

    Ok(resp.data)
}

pub async fn create_doc(title: &str, text: &str) -> Result<OutlineDoc, crate::Error> {
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/documents.create", &*OUTLINE_URL))
        .header("Authorization", auth_header())
        .json(&serde_json::json!({
            "title": title,
            "text": text,
            "collectionId": &*OUTLINE_COLLECTION_ID,
            "publish": true
        }))
        .send()
        .await?
        .json::<DocResponse>()
        .await?;

    Ok(resp.data)
}

pub async fn update_doc(id: &str, title: &str, text: &str) -> Result<OutlineDoc, crate::Error> {
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/documents.update", &*OUTLINE_URL))
        .header("Authorization", auth_header())
        .json(&serde_json::json!({
            "id": id,
            "title": title,
            "text": text,
            "publish": true
        }))
        .send()
        .await?
        .json::<DocResponse>()
        .await?;

    Ok(resp.data)
}
