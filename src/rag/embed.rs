use serde::{Deserialize, Serialize};

use crate::env::{EMBEDDING_MODEL, LOCALAI_URL};
use crate::{Error, HTTP_CLIENT};

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

pub async fn embed(text: &str) -> Result<Vec<f32>, Error> {
    embed_batch(&[text])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "Empty embedding response from Ollama".into())
}

pub async fn embed_batch(texts: &[&str]) -> Result<Vec<Vec<f32>>, Error> {
    if texts.is_empty() {
        return Ok(vec![]);
    }
    let client = HTTP_CLIENT.get().unwrap();
    let resp = client
        .post(format!("{}/api/embed", &*LOCALAI_URL))
        .json(&EmbedRequest {
            model: &EMBEDDING_MODEL,
            input: texts.to_vec(),
        })
        .send()
        .await?
        .json::<EmbedResponse>()
        .await?;
    Ok(resp.embeddings)
}
