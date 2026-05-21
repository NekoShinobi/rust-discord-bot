use std::sync::Arc;

use chrono::Utc;
use poise::serenity_prelude as serenity;
use serenity::builder::GetMessages;
use serenity::http::Http;
use serenity::model::id::{ChannelId, MessageId};

use crate::rag::{self, ingest, qdrant, ChannelState};
use crate::rag::qdrant::MessagePoint;

const IDLE_THRESHOLD_SECS: i64 = 1800;   // channel must be idle 30 min before backfill
const POLL_INTERVAL_SECS: u64 = 300;    // check every 5 min
const BACKFILL_COOLDOWN_SECS: i64 = 300; // one batch per channel per 5 min

pub async fn backfill_loop(http: Arc<Http>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let channels = rag::all_registered_channels();
        for (channel_id, state) in channels {
            if state.backfill_complete {
                continue;
            }

            let idle_secs = (Utc::now() - state.last_message_at).num_seconds();
            if idle_secs < IDLE_THRESHOLD_SECS {
                continue;
            }

            if let Some(last_attempt) = state.last_backfill_attempt {
                if (Utc::now() - last_attempt).num_seconds() < BACKFILL_COOLDOWN_SECS {
                    continue;
                }
            }

            log::info!("Starting backfill for channel {}", channel_id);
            if let Err(e) = backfill_channel(&http, channel_id, state).await {
                log::warn!("Backfill failed for channel {}: {:?}", channel_id, e);
            }
            // Avoid back-to-back Discord API calls across multiple channels
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }
}

async fn backfill_channel(
    http: &Arc<Http>,
    channel_id: u64,
    mut state: ChannelState,
) -> Result<(), crate::Error> {
    state.last_backfill_attempt = Some(Utc::now());
    rag::save_channel_state(channel_id, &state)?;

    let serenity_channel = ChannelId::new(channel_id);
    let mut builder = GetMessages::new().limit(100);
    if let Some(oldest_id) = state.oldest_indexed_message_id {
        builder = builder.before(MessageId::new(oldest_id));
    }

    let messages = serenity_channel.messages(http, builder).await?;

    if messages.is_empty() {
        state.backfill_complete = true;
        rag::save_channel_state(channel_id, &state)?;
        log::info!("Backfill complete for channel {}", channel_id);
        return Ok(());
    }

    // Filter out messages we don't want to index
    let valid: Vec<_> = messages
        .iter()
        .filter(|m| {
            let content = m.content.trim();
            !content.is_empty() && !ingest::should_skip(content, m.author.bot)
        })
        .collect();

    if !valid.is_empty() {
        // Enrich each message with key concepts then batch embed
        let mut indexable: Vec<(&serenity::Message, String, Option<String>)> = Vec::new();
        for m in &valid {
            let content = m.content.trim();
            let tags = crate::rag::enrich::enrich(content).await;
            let embed_text = if tags.is_empty() {
                format!("{}: {}", m.author.name, content)
            } else {
                format!("{}: {} | {}", m.author.name, content, tags)
            };
            indexable.push((m, embed_text, if tags.is_empty() { None } else { Some(tags) }));
        }
        let text_refs: Vec<&str> = indexable.iter().map(|(_, t, _)| t.as_str()).collect();

        match crate::rag::embed::embed_batch(&text_refs).await {
            Ok(embeddings) => {
                let points: Vec<MessagePoint> = indexable
                    .into_iter()
                    .zip(embeddings)
                    .map(|((msg, _, tags), emb)| MessagePoint {
                        message_id: msg.id.get(),
                        channel_id,
                        author_name: msg.author.name.clone(),
                        content: msg.content.trim().to_string(),
                        timestamp: msg.timestamp.unix_timestamp(),
                        tags,
                        embedding: emb,
                    })
                    .collect();

                if let Err(e) = qdrant::upsert_batch(points).await {
                    log::warn!("Backfill batch upsert failed for channel {}: {:?}", channel_id, e);
                }
            }
            Err(e) => {
                log::warn!("Backfill batch embed failed for channel {}: {:?}", channel_id, e);
            }
        }
    }

    // Update channel state with the oldest message ID from this batch
    if let Some(updated_state) = rag::get_channel_state(channel_id) {
        let mut new_state = updated_state;
        let batch_oldest = messages.iter().map(|m| m.id.get()).min();
        if let Some(oldest) = batch_oldest {
            if new_state.oldest_indexed_message_id.map_or(true, |id| oldest < id) {
                new_state.oldest_indexed_message_id = Some(oldest);
            }
        }
        rag::save_channel_state(channel_id, &new_state)?;
    }

    log::info!(
        "Backfill batch done for channel {}: {}/{} messages embedded",
        channel_id,
        valid.len(),
        messages.len(),
    );

    Ok(())
}
