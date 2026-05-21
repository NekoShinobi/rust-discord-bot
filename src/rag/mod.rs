pub mod backfill;
pub mod commands;
pub mod embed;
pub mod enrich;
pub mod ingest;
pub mod qdrant;
pub mod retrieval;

use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use redb::ReadableDatabase;

use crate::{Error, KV_DATABASE, RAG_CHANNELS};

pub static QDRANT_CLIENT: OnceLock<qdrant_client::Qdrant> = OnceLock::new();

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChannelState {
    pub registered_at: DateTime<Utc>,
    pub oldest_indexed_message_id: Option<u64>,
    pub latest_indexed_message_id: Option<u64>,
    pub last_message_at: DateTime<Utc>,
    pub last_backfill_attempt: Option<DateTime<Utc>>,
    pub backfill_complete: bool,
}

pub async fn init() -> Result<(), Error> {
    qdrant::init().await
}

pub fn is_channel_registered(channel_id: u64) -> bool {
    let Some(db) = KV_DATABASE.get() else {
        return false;
    };
    let Ok(rx) = db.begin_read() else { return false };
    let Ok(table) = rx.open_table(RAG_CHANNELS) else {
        return false;
    };
    table
        .get(channel_id.to_string().as_str())
        .ok()
        .flatten()
        .is_some()
}

pub fn get_channel_state(channel_id: u64) -> Option<ChannelState> {
    let db = KV_DATABASE.get()?;
    let rx = db.begin_read().ok()?;
    let table = rx.open_table(RAG_CHANNELS).ok()?;
    let value = table.get(channel_id.to_string().as_str()).ok()??;
    serde_json::from_str(value.value()).ok()
}

pub fn save_channel_state(channel_id: u64, state: &ChannelState) -> Result<(), Error> {
    let db = KV_DATABASE.get().ok_or("DB not initialized")?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(RAG_CHANNELS)?;
        let data = serde_json::to_string(state)?;
        table.insert(channel_id.to_string().as_str(), data.as_str())?;
    }
    tx.commit()?;
    Ok(())
}

pub fn all_registered_channels() -> Vec<(u64, ChannelState)> {
    let Some(db) = KV_DATABASE.get() else {
        return vec![];
    };
    let Ok(rx) = db.begin_read() else { return vec![] };
    let Ok(table) = rx.open_table(RAG_CHANNELS) else {
        return vec![];
    };
    table
        .range::<&str>(..)
        .ok()
        .map(|iter| {
            iter.filter_map(|item| {
                let (k, v) = item.ok()?;
                let channel_id: u64 = k.value().parse().ok()?;
                let state: ChannelState = serde_json::from_str(v.value()).ok()?;
                Some((channel_id, state))
            })
            .collect()
        })
        .unwrap_or_default()
}
