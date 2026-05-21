use chrono::Utc;
use poise::serenity_prelude as serenity;
use regex::Regex;
use std::sync::LazyLock;

use crate::rag::{self, ChannelState};
use crate::Error;

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://\S+").unwrap());

const MIN_CONTENT_LEN: usize = 5;

pub(crate) fn should_skip(content: &str, is_bot: bool) -> bool {
    // Skip command invocations from non-bot users
    if !is_bot && (content.starts_with('/') || content.starts_with('~')) {
        return true;
    }

    // Skip messages that are only a URL (no surrounding text)
    let stripped = URL_RE.replace_all(content, "");
    if stripped.trim().is_empty() {
        return true;
    }

    // Skip very short messages
    if content.len() < MIN_CONTENT_LEN {
        return true;
    }

    // Skip messages with fewer than 3 distinct characters (e.g. ".....", "!!!", "~~~")
    let unique: std::collections::HashSet<char> = content.chars().collect();
    if unique.len() < 3 {
        return true;
    }

    false
}

pub async fn ingest_message(msg: serenity::Message) -> Result<(), Error> {
    let content = msg.content.trim().to_string();
    if content.is_empty() || should_skip(&content, msg.author.bot) {
        return Ok(());
    }

    ingest_raw(
        msg.id.get(),
        msg.channel_id.get(),
        &msg.author.name,
        &content,
        msg.timestamp.unix_timestamp(),
    )
    .await
}

pub async fn ingest_raw(
    message_id: u64,
    channel_id: u64,
    author_name: &str,
    content: &str,
    timestamp: i64,
) -> Result<(), Error> {
    let tags = crate::rag::enrich::enrich(content).await;

    let embed_text = if tags.is_empty() {
        format!("{}: {}", author_name, content)
    } else {
        format!("{}: {} | {}", author_name, content, tags)
    };

    let embedding = match crate::rag::embed::embed(&embed_text).await {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Failed to embed message {}: {:?}", message_id, e);
            return Ok(());
        }
    };

    let tags_opt = if tags.is_empty() { None } else { Some(tags.as_str()) };
    if let Err(e) =
        crate::rag::qdrant::upsert_message(message_id, channel_id, author_name, content, timestamp, tags_opt, embedding).await
    {
        log::warn!("Failed to upsert message {} to qdrant: {:?}", message_id, e);
        return Ok(());
    }

    let mut state = rag::get_channel_state(channel_id).unwrap_or_else(|| ChannelState {
        registered_at: Utc::now(),
        oldest_indexed_message_id: Some(message_id),
        latest_indexed_message_id: Some(message_id),
        last_message_at: Utc::now(),
        last_backfill_attempt: None,
        backfill_complete: false,
    });

    state.last_message_at = Utc::now();
    if state.latest_indexed_message_id.map_or(true, |id| message_id > id) {
        state.latest_indexed_message_id = Some(message_id);
    }
    if state.oldest_indexed_message_id.map_or(true, |id| message_id < id) {
        state.oldest_indexed_message_id = Some(message_id);
    }

    if let Err(e) = rag::save_channel_state(channel_id, &state) {
        log::warn!("Failed to save channel state for {}: {:?}", channel_id, e);
    }

    Ok(())
}
