use chrono::{TimeZone, Utc};

use crate::ai::localai::ModelMessageData;

const MIN_RAG_SCORE: f32 = 0.6;

pub async fn hybrid_context(
    channel_id: u64,
    query: &str,
    recent_msgs: &[ModelMessageData],
) -> Vec<ModelMessageData> {
    let expanded = expand_query(query, recent_msgs).await;
    let search_query = if expanded.is_empty() {
        query.to_string()
    } else {
        format!("{} {}", query, expanded)
    };

    let embedding = match crate::rag::embed::embed(&search_query).await {
        Ok(e) => e,
        Err(e) => {
            log::warn!("RAG embed failed: {:?}", e);
            return vec![];
        }
    };

    let mut results = match crate::rag::qdrant::search(channel_id, embedding, 60).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("RAG search failed for channel {}: {:?}", channel_id, e);
            return vec![];
        }
    };

    results.retain(|r| r.score >= MIN_RAG_SCORE);

    if results.is_empty() {
        return vec![];
    }

    // Deduplicate near-identical results — keeps the highest-scoring copy.
    // Results are already score-sorted descending from qdrant, so first occurrence wins.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    results.retain(|r| {
        let key = r.content.to_lowercase();
        let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
        seen.insert(key)
    });

    results.truncate(30);
    results.sort_by_key(|r| r.timestamp);

    results
        .into_iter()
        .map(|r| {
            let ts = Utc
                .timestamp_opt(r.timestamp, 0)
                .single()
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "unknown time".to_string());
            ModelMessageData {
                role: "system".to_string(),
                content: format!("[{} at {}]: {}", r.author_name, ts, r.content),
                images: None,
            }
        })
        .collect()
}

async fn expand_query(query: &str, recent_msgs: &[ModelMessageData]) -> String {
    let context_str = recent_msgs
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let user_content = if context_str.is_empty() {
        format!("Message: {}", query)
    } else {
        format!("Recent conversation:\n{}\n\nNew message: {}", context_str, query)
    };

    let body = serde_json::json!({
        "model": &*crate::env::QUERY_EXPAND_MODEL,
        "messages": [
            {
                "role": "system",
                "content": "You are a search query expander. Given a message and optional conversation context, output 5-10 specific search terms, synonyms, or concrete examples that capture what the user is referring to. Respond with ONLY a comma-separated list of terms, nothing else."
            },
            {
                "role": "user",
                "content": user_content
            }
        ],
        "stream": false,
        "options": {
            "temperature": 0.2,
            "num_predict": 80
        }
    });

    let resp = match crate::HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/chat", &*crate::env::LOCALAI_URL))
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Query expansion request failed: {:?}", e);
            return String::new();
        }
    };

    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Query expansion response read failed: {:?}", e);
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
                if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_str()) {
                    let expanded = content.trim().to_string();
                    if !expanded.is_empty() {
                        log::info!("RAG query expansion: {:?} -> {:?}", query, expanded);
                        return expanded;
                    }
                }
            }
        }
    }

    String::new()
}
