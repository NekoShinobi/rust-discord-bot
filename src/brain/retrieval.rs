use crate::ai::localai::ModelMessageData;
use crate::brain::outline;
use crate::{BRAIN_INDEX, KV_DATABASE};
use redb::ReadableDatabase;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BrainIndexEntry {
    pub id: String,
    pub title: String,
}

/// Search Outline for docs relevant to `query`, return as system context messages.
pub async fn fetch_context(query: &str) -> Vec<ModelMessageData> {
    match outline::search(query, 3).await {
        Ok(docs) if !docs.is_empty() => docs
            .into_iter()
            .map(|doc| {
                let snippet = if doc.text.len() > 800 {
                    format!("{}…", &doc.text[..800])
                } else {
                    doc.text.clone()
                };
                ModelMessageData {
                    role: "system".to_string(),
                    content: format!("[Memory: {}]\n{}", doc.title, snippet),
                    images: None,
                }
            })
            .collect(),
        Ok(_) => vec![],
        Err(e) => {
            log::warn!("Brain retrieval failed: {:?}", e);
            vec![]
        }
    }
}

/// Create or append to an Outline doc. Looks up existing doc by title in local index.
pub async fn save_memory(title: &str, content: &str) {
    let existing_id = lookup_index(title);

    let result = if let Some(id) = existing_id {
        match outline::get_doc(&id).await {
            Ok(doc) => {
                let updated_text = format!("{}\n\n{}", doc.text.trim(), content.trim());
                outline::update_doc(&id, title, &updated_text).await
            }
            Err(e) => {
                log::warn!("Brain: failed to fetch existing doc '{}': {:?}", title, e);
                outline::create_doc(title, content).await
            }
        }
    } else {
        outline::create_doc(title, content).await
    };

    match result {
        Ok(doc) => {
            upsert_index(&doc.id, &doc.title);
            log::info!("Brain: saved memory '{}' (id: {})", doc.title, doc.id);
        }
        Err(e) => {
            log::warn!("Brain: failed to save memory '{}': {:?}", title, e);
        }
    }
}

fn lookup_index(title: &str) -> Option<String> {
    let db = KV_DATABASE.get()?;
    let rx = db.begin_read().ok()?;
    let table = rx.open_table(BRAIN_INDEX).ok()?;
    let key = title.to_lowercase();
    let value = table.get(key.as_str()).ok()??;
    let entry: BrainIndexEntry = serde_json::from_str(value.value()).ok()?;
    Some(entry.id)
}

fn upsert_index(id: &str, title: &str) {
    let Some(db) = KV_DATABASE.get() else { return };
    let Ok(tx) = db.begin_write() else { return };
    {
        let Ok(mut table) = tx.open_table(BRAIN_INDEX) else { return };
        let entry = BrainIndexEntry { id: id.to_string(), title: title.to_string() };
        let Ok(data) = serde_json::to_string(&entry) else { return };
        let key = title.to_lowercase();
        let _ = table.insert(key.as_str(), data.as_str());
    }
    let _ = tx.commit();
}
