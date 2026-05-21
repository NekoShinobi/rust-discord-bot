use chrono::{TimeZone, Utc};
use poise::serenity_prelude as serenity;

use crate::rag::{self, ChannelState};
use crate::{Context, Error, KV_DATABASE, RAG_CHANNELS};

/// Register this channel for AI context indexing
#[poise::command(
    slash_command,
    prefix_command,
    check = "crate::permissions::check_admin",
    category = "AI"
)]
pub async fn rag_register(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();

    if rag::is_channel_registered(channel_id) {
        ctx.say("Channel is already registered for indexing.").await?;
        return Ok(());
    }

    let state = ChannelState {
        registered_at: Utc::now(),
        oldest_indexed_message_id: None,
        latest_indexed_message_id: None,
        last_message_at: Utc::now(),
        last_backfill_attempt: None,
        backfill_complete: false,
    };

    rag::save_channel_state(channel_id, &state)?;
    ctx.say(
        "Channel registered. New messages will be indexed immediately. \
        Historical backfill starts once the channel has been idle for 30 minutes.",
    )
    .await?;
    Ok(())
}

/// Unregister this channel from AI context indexing
#[poise::command(
    slash_command,
    prefix_command,
    check = "crate::permissions::check_admin",
    category = "AI"
)]
pub async fn rag_unregister(
    ctx: Context<'_>,
    #[description = "Also delete all indexed vector data for this channel"]
    delete_data: Option<bool>,
) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();

    if !rag::is_channel_registered(channel_id) {
        ctx.say("This channel is not registered.").await?;
        return Ok(());
    }

    let db = KV_DATABASE.get().ok_or("DB not initialized")?;
    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(RAG_CHANNELS)?;
        table.remove(channel_id.to_string().as_str())?;
    }
    tx.commit()?;

    if delete_data.unwrap_or(false) {
        if let Err(e) = rag::qdrant::delete_channel(channel_id).await {
            log::warn!(
                "Failed to delete qdrant data for channel {}: {:?}",
                channel_id,
                e
            );
            ctx.say("Unregistered, but failed to delete vector data — check logs.").await?;
            return Ok(());
        }
        ctx.say("Channel unregistered and all indexed data deleted.").await?;
    } else {
        ctx.say("Channel unregistered. Vector data retained (pass `delete_data: true` to remove it).").await?;
    }

    Ok(())
}

/// Search the RAG index and show similarity results without an AI response
#[poise::command(
    slash_command,
    prefix_command,
    check = "crate::permissions::check_admin",
    category = "AI"
)]
pub async fn rag_search(
    ctx: Context<'_>,
    #[description = "Search query"] query: String,
) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();

    if !rag::is_channel_registered(channel_id) {
        ctx.say("This channel is not registered for indexing.").await?;
        return Ok(());
    }

    ctx.defer().await?;

    // Enrich the query the same way messages are enriched at ingest time
    let tags = rag::enrich::enrich(&query).await;
    let enriched_query = if tags.is_empty() {
        query.clone()
    } else {
        format!("{} | {}", query, tags)
    };

    let embedding = match rag::embed::embed(&enriched_query).await {
        Ok(e) => e,
        Err(e) => {
            log::warn!("rag_search embed failed: {:?}", e);
            ctx.say("Failed to embed query.").await?;
            return Ok(());
        }
    };

    let mut results = match rag::qdrant::search(channel_id, embedding, 60).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("rag_search qdrant search failed: {:?}", e);
            ctx.say("Search failed.").await?;
            return Ok(());
        }
    };

    results.retain(|r| r.score >= 0.6);

    // Deduplicate near-identical results
    let mut seen = std::collections::HashSet::new();
    results.retain(|r| {
        let key = r.content.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
        seen.insert(key)
    });

    results.truncate(30);

    if results.is_empty() {
        ctx.say("No results found.").await?;
        return Ok(());
    }

    let description = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let content = if r.content.len() > 200 {
                format!("{}…", &r.content[..200])
            } else {
                r.content.clone()
            };
            let ts = chrono::Utc
                .timestamp_opt(r.timestamp, 0)
                .single()
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            format!("`{}` **{}** at {}: {}", i + 1, r.author_name, ts, content)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Discord embed description limit is 4096 chars
    let description = if description.len() > 4096 {
        description[..4096].to_string()
    } else {
        description
    };

    let footer = if tags.is_empty() {
        format!("{} results", results.len())
    } else {
        format!("{} results • tags: {}", results.len(), tags)
    };

    let embed = serenity::CreateEmbed::new()
        .title(format!("RAG Search: {}", query))
        .description(description)
        .footer(serenity::CreateEmbedFooter::new(footer))
        .color(0x5865F2u32);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Show RAG indexing status for this channel
#[poise::command(
    slash_command,
    prefix_command,
    check = "crate::permissions::check_admin",
    category = "AI"
)]
pub async fn rag_status(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();

    let Some(state) = rag::get_channel_state(channel_id) else {
        ctx.say("This channel is not registered for indexing.").await?;
        return Ok(());
    };

    let point_count = rag::qdrant::count_channel(channel_id).await.unwrap_or(0);

    let backfill_status = if state.backfill_complete {
        "complete"
    } else if state.last_backfill_attempt.is_some() {
        "in progress"
    } else {
        "pending (channel must be idle ≥30 min)"
    };

    ctx.say(format!(
        "**RAG Status**\nRegistered: {}\nIndexed messages: {}\nBackfill: {}\nLast activity: {}",
        state.registered_at.format("%Y-%m-%d %H:%M UTC"),
        point_count,
        backfill_status,
        state.last_message_at.format("%Y-%m-%d %H:%M UTC"),
    ))
    .await?;

    Ok(())
}
