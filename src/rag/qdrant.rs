use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::{
    Condition, CountPointsBuilder, CreateCollectionBuilder, CreateFieldIndexCollectionBuilder,
    DeletePointsBuilder, Distance, FieldType, Filter, PointStruct, SearchPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::Payload;
use qdrant_client::Qdrant;

use crate::env::{EMBEDDING_DIM, QDRANT_URL};
use crate::rag::QDRANT_CLIENT;
use crate::Error;

const COLLECTION: &str = "discord_messages";

pub struct MessagePoint {
    pub message_id: u64,
    pub channel_id: u64,
    pub author_name: String,
    pub content: String,
    pub timestamp: i64,
    pub tags: Option<String>,
    pub embedding: Vec<f32>,
}

pub struct SearchResult {
    pub message_id: u64,
    pub author_name: String,
    pub content: String,
    pub timestamp: i64,
    pub score: f32,
}

pub async fn init() -> Result<(), Error> {
    let client = Qdrant::from_url(&QDRANT_URL).build()?;

    if !client.collection_exists(COLLECTION).await? {
        client
            .create_collection(
                CreateCollectionBuilder::new(COLLECTION)
                    .vectors_config(VectorParamsBuilder::new(*EMBEDDING_DIM, Distance::Cosine)),
            )
            .await?;

        client
            .create_field_index(CreateFieldIndexCollectionBuilder::new(
                COLLECTION,
                "channel_id",
                FieldType::Keyword,
            ))
            .await?;

        client
            .create_field_index(CreateFieldIndexCollectionBuilder::new(
                COLLECTION,
                "timestamp",
                FieldType::Integer,
            ))
            .await?;

        log::info!("Created qdrant collection '{}'", COLLECTION);
    }

    let _ = QDRANT_CLIENT.set(client);
    Ok(())
}

pub async fn upsert_message(
    message_id: u64,
    channel_id: u64,
    author_name: &str,
    content: &str,
    timestamp: i64,
    tags: Option<&str>,
    embedding: Vec<f32>,
) -> Result<(), Error> {
    let client = match QDRANT_CLIENT.get() {
        Some(c) => c,
        None => return Ok(()),
    };

    let mut payload = Payload::new();
    payload.insert("channel_id", channel_id.to_string());
    payload.insert("message_id", message_id.to_string());
    payload.insert("author_name", author_name.to_string());
    payload.insert("content", content.to_string());
    payload.insert("timestamp", timestamp);
    if let Some(t) = tags {
        payload.insert("tags", t.to_string());
    }

    client
        .upsert_points(UpsertPointsBuilder::new(
            COLLECTION,
            vec![PointStruct::new(message_id, embedding, payload)],
        ))
        .await?;

    Ok(())
}

pub async fn upsert_batch(messages: Vec<MessagePoint>) -> Result<(), Error> {
    let client = match QDRANT_CLIENT.get() {
        Some(c) => c,
        None => return Ok(()),
    };

    let points: Vec<PointStruct> = messages
        .into_iter()
        .map(|m| {
            let mut payload = Payload::new();
            payload.insert("channel_id", m.channel_id.to_string());
            payload.insert("message_id", m.message_id.to_string());
            payload.insert("author_name", m.author_name);
            payload.insert("content", m.content);
            payload.insert("timestamp", m.timestamp);
            if let Some(t) = m.tags {
                payload.insert("tags", t);
            }
            PointStruct::new(m.message_id, m.embedding, payload)
        })
        .collect();

    client
        .upsert_points(UpsertPointsBuilder::new(COLLECTION, points))
        .await?;

    Ok(())
}

pub async fn search(
    channel_id: u64,
    embedding: Vec<f32>,
    limit: u64,
) -> Result<Vec<SearchResult>, Error> {
    let client = match QDRANT_CLIENT.get() {
        Some(c) => c,
        None => return Ok(vec![]),
    };

    let response = client
        .search_points(
            SearchPointsBuilder::new(COLLECTION, embedding, limit)
                .filter(Filter::must([Condition::matches(
                    "channel_id",
                    channel_id.to_string(),
                )]))
                .with_payload(true),
        )
        .await?;

    let results = response
        .result
        .into_iter()
        .filter_map(|point| {
            let payload = point.payload;

            let author_name = payload
                .get("author_name")
                .and_then(|v| match &v.kind {
                    Some(qdrant_client::qdrant::value::Kind::StringValue(s)) => Some(s.clone()),
                    _ => None,
                })?;

            let content = payload
                .get("content")
                .and_then(|v| match &v.kind {
                    Some(qdrant_client::qdrant::value::Kind::StringValue(s)) => Some(s.clone()),
                    _ => None,
                })?;

            let timestamp = payload
                .get("timestamp")
                .and_then(|v| match &v.kind {
                    Some(qdrant_client::qdrant::value::Kind::IntegerValue(i)) => Some(*i),
                    _ => None,
                })
                .unwrap_or(0);

            let message_id = match point.id?.point_id_options? {
                PointIdOptions::Num(n) => n,
                _ => return None,
            };

            Some(SearchResult {
                message_id,
                author_name,
                content,
                timestamp,
                score: point.score,
            })
        })
        .collect();

    Ok(results)
}

pub async fn delete_channel(channel_id: u64) -> Result<(), Error> {
    let client = match QDRANT_CLIENT.get() {
        Some(c) => c,
        None => return Ok(()),
    };

    client
        .delete_points(
            DeletePointsBuilder::new(COLLECTION).points(Filter::must([Condition::matches(
                "channel_id",
                channel_id.to_string(),
            )])),
        )
        .await?;

    Ok(())
}

pub async fn count_channel(channel_id: u64) -> Result<u64, Error> {
    let client = match QDRANT_CLIENT.get() {
        Some(c) => c,
        None => return Ok(0),
    };

    let response = client
        .count(
            CountPointsBuilder::new(COLLECTION)
                .filter(Filter::must([Condition::matches(
                    "channel_id",
                    channel_id.to_string(),
                )]))
                .exact(true),
        )
        .await?;

    Ok(response.result.map(|r| r.count).unwrap_or(0))
}
