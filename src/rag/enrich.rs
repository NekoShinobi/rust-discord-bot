use crate::env::{LOCALAI_URL, QUERY_EXPAND_MODEL};
use crate::HTTP_CLIENT;

/// Returns extracted tags for the message, or an empty string if none found or on error.
pub async fn enrich(content: &str) -> String {
    let body = serde_json::json!({
        "model": &*QUERY_EXPAND_MODEL,
        "messages": [
            {
                "role": "system",
                "content": "Extract 5-10 key concepts, entities, topics, and related terms from the message. Return ONLY a comma-separated list of terms, nothing else."
            },
            {
                "role": "user",
                "content": content
            }
        ],
        "stream": false,
        "options": {
            "temperature": 0.1,
            "num_predict": 60
        }
    });

    let resp = match HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/chat", &*LOCALAI_URL))
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Enrichment request failed: {:?}", e);
            return String::new();
        }
    };

    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Enrichment response read failed: {:?}", e);
            return String::new();
        }
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                if let Some(c) = v.pointer("/message/content").and_then(|c| c.as_str()) {
                    return c.trim().to_string();
                }
            }
        }
    }

    log::warn!("Enrich produced no tags for {:?} using model {:?}. Raw response: {:?}", content, &*QUERY_EXPAND_MODEL, text);
    String::new()
}
