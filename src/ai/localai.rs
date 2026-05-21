// https://github.com/ollama/ollama/blob/main/docs/api.md

use std::collections::VecDeque;
use std::sync::LazyLock;

use tokio::sync::Semaphore;

use crate::env::LOCALAI_URL;
use crate::{AI_CONTEXT, Error, HTTP_CLIENT, KV_DATABASE};

use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, TimeZone, Utc};
use poise::serenity_prelude as serenity;
use redb::ReadableDatabase;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelMessageData {
    pub role: String,
    pub content: String,
    pub images: Option<Vec<String>>,
}

impl Default for ModelMessageData {
    fn default() -> Self {
        ModelMessageData {
            role: "system".to_string(),
            content: "Given the following conversation, relevant context, and a follow up question, reply with an answer to the current question the user is asking. Return only your response to the question given the above information following the users instructions as needed.".to_string(),
            images: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaOptions {
    pub num_ctx: u64,
    pub temperature: f64,
    pub num_predict: u64,
    pub repeat_penalty: f64,
    pub repeat_last_n: i64,
}

impl Default for OllamaOptions {
    fn default() -> Self {
        OllamaOptions {
            num_ctx: 16384,
            temperature: 0.5,
            num_predict: 768,
            repeat_penalty: 1.3,
            repeat_last_n: 512,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelData {
    pub model: String,
    pub messages: VecDeque<ModelMessageData>,
    pub options: OllamaOptions,
    pub stream: bool,
    pub think: bool,
    pub format: serde_json::Value,
}

impl ModelData {
    fn new() -> Self {
        ModelData::default()
    }
}

impl Default for ModelData {
    fn default() -> Self {
        ModelData {
            model: "gemma4:26b".to_string(), // TODO: Configurable
            messages: VecDeque::new(),
            options: OllamaOptions::default(),
            stream: false,
            think: false,
            format: RIN_RESPONSE_SCHEMA.clone(),
        }
    }
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct ModelResponseMessage {
//     pub content: String,
//     pub role: String,
// }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct ModelResponseChoice {
//     pub message: ModelResponseMessage,
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelResponse {
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub message: ModelMessageData,
    pub done_reason: Option<String>,
    pub done: bool,
    pub total_duration: Option<u64>,
    pub load_duration: Option<u64>,
    pub prompt_eval_count: Option<u64>,
    pub prompt_eval_duration: Option<u64>,
    pub eval_count: Option<u64>,
    pub eval_duration: Option<u64>,
}

static OLLAMA_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbedData {
    pub title: Option<String>,
    pub description: Option<String>,
    pub fields: Option<Vec<EmbedField>>,
    pub footer: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryWrite {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RinResponse {
    pub text: String,
    pub embed: Option<EmbedData>,
    pub react_user: Option<String>,
    pub react_self: Option<String>,
    pub memory: Option<MemoryWrite>,
}

static RIN_RESPONSE_SCHEMA: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::json!({
        "type": "object",
        "properties": {
            "text": {"type": "string"},
            "embed": {
                "type": ["object", "null"],
                "properties": {
                    "title":       {"type": ["string", "null"]},
                    "description": {"type": ["string", "null"]},
                    "fields": {
                        "type": ["array", "null"],
                        "maxItems": 5,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name":  {"type": "string"},
                                "value": {"type": "string"}
                            },
                            "required": ["name", "value"]
                        }
                    },
                    "footer": {"type": ["string", "null"]}
                }
            },
            "react_user": {"type": ["string", "null"]},
            "react_self":  {"type": ["string", "null"]},
            "memory": {
                "type": ["object", "null"],
                "properties": {
                    "title":   {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["title", "content"]
            }
        },
        "required": ["text", "embed", "react_user", "react_self", "memory"]
    })
});

pub async fn wipe_context(message: &serenity::Message) -> Result<(), Error> {
    let db = KV_DATABASE.get().unwrap();
    let tx = db.begin_write()?;
    {
        let mut tx_table = tx.open_table(AI_CONTEXT)?;
        let channel_id: &str = &message.channel_id.to_string();
        tx_table.remove(channel_id)?;
    }
    tx.commit()?;
    log::info!("Context Cleared for {:?}", message.channel_id);

    Ok(())
}

pub async fn get_gpt_response(
    message: &serenity::Message,
    ctx: &serenity::Context,
    system_prompt: &str,
) -> Result<RinResponse, Error> {
    let db = KV_DATABASE.get().unwrap();
    let rx = db.begin_read()?;
    let rx_table = rx.open_table(AI_CONTEXT)?;

    let mut map = ModelData::new();

    let channel_id: &str = &message.channel_id.to_string();

    // Load stored conversation first — needed to pass recent context to query expander.
    let mut stored_messages: VecDeque<ModelMessageData> =
        if let Some(stored_value) = rx_table.get(channel_id)? {
            serde_json::from_str(stored_value.value()).unwrap_or_default()
        } else {
            VecDeque::new()
        };

    // System prompt goes first
    map.messages.push_back(ModelMessageData {
        role: "system".to_string(),
        content: format!(
            "{}\n\nRespond using the JSON schema provided.\
            \nThe `text` field must be plain text only — no JSON, no large amount of newlines.\
            \nAlways populate `text` with your main response.\
            \n\
            \nThe `embed` field renders a rich Discord embed card alongside your text. Use it when presenting structured or visual information:\
            \n- `title`: a short heading for the card (optional)\
            \n- `description`: a paragraph of detail or summary (optional)\
            \n- `fields`: up to 5 named sections, each with a `name` label and `value` body — good for comparisons, stats, lists\
            \n- `footer`: a small note at the bottom (optional)\
            \nOnly use embed when it genuinely adds structure. For plain conversational replies, set embed to null.\
            \n\
            \nUse `react_user` to react to the user's message with an emoji when appropriate — otherwise null.\
            \nUse `react_self` to annotate your own reply's tone with an emoji — otherwise null.\
            \nUse `memory` to save something worth remembering long-term (user preferences, facts, decisions) — set `title` as a short topic and `content` as the fact. Use null if nothing is worth saving.",
            system_prompt
        ),
        images: None,
    });

    // Brain (Outline) context — long-term memory docs, injected before conversation.
    let mut rag_prefix_len: usize = 1; // system prompt
    if crate::brain::is_enabled() {
        let query = message.content_safe(&ctx.cache);
        let brain_msgs = crate::brain::retrieval::fetch_context(&query).await;
        if !brain_msgs.is_empty() {
            let brain_block = brain_msgs
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            map.messages.push_back(ModelMessageData {
                role: "system".to_string(),
                content: format!(
                    "The following are relevant long-term memories. Use them as background context.\n\n{}",
                    brain_block
                ),
                images: None,
            });
            rag_prefix_len += 1;
        }
    }

    // RAG context as a single system message block, before the conversation thread.
    let channel_id_u64: u64 = message.channel_id.get();
    if crate::rag::is_channel_registered(channel_id_u64) {
        let query = message.content_safe(&ctx.cache);
        // Pass last 3 stored messages as context for query expansion.
        let recent_context: Vec<ModelMessageData> = stored_messages
            .iter()
            .rev()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let rag_messages =
            crate::rag::retrieval::hybrid_context(channel_id_u64, &query, &recent_context).await;
        if !rag_messages.is_empty() {
            let rag_block = rag_messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            map.messages.push_back(ModelMessageData {
                role: "system".to_string(),
                content: format!(
                    "The following are semantically relevant past messages from this channel. \
                     Use them as background context if helpful, but do not treat them as part \
                     of the current conversation. Timestamps indicate when each was sent.\n\n{}",
                    rag_block
                ),
                images: None,
            });
            rag_prefix_len += 1;
        }
    }

    // Append recent conversation thread
    map.messages.append(&mut stored_messages);

    let mut images = Vec::new();
    if !message.attachments.is_empty() {
        println!("{:?}", message.attachments);
        let content = match message.attachments[0].download().await {
            Ok(content) => general_purpose::STANDARD.encode(content),
            Err(why) => {
                println!("Error downloading attachment: {:?}", why);
                let _ = message
                    .channel_id
                    .say(&ctx, "Error downloading attachment")
                    .await;
                String::new()
            }
        };
        if !content.is_empty() {
            images.push(content)
        }
    }

    let msg_ts = chrono::Utc
        .timestamp_opt(message.timestamp.unix_timestamp(), 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "unknown time".to_string());

    let new_msg = ModelMessageData {
        role: "user".to_string(),
        content: format!(
            "[{}] {} says: {}",
            msg_ts,
            message.author.name,
            message
                .content_safe(&ctx.cache)
                .replace("@Rin#7236", "")
                .trim_start(),
        ),
        images: Some(images),
    };
    map.messages.push_back(new_msg);

    log::info!("GPT Sent ({} messages) {:#?}", map.messages.len(), map.messages);
    let _permit = OLLAMA_SEMAPHORE.acquire().await?;
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/chat", &*LOCALAI_URL))
        .json(&map)
        .send()
        .await?;

    let json_string = resp.text().await?;
    // Thinking models (e.g. gemma4) emit a done:false thinking chunk then a done:true final chunk
    // as NDJSON even with stream:false. Parse all lines, take the last done:true object.
    let model_response: ModelResponse = {
        let mut final_response: Option<ModelResponse> = None;
        for line in json_string.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<ModelResponse>(line) {
                Ok(r) if r.done => {
                    final_response = Some(r);
                }
                Ok(_) => {} // done:false thinking chunk, skip
                Err(why) => {
                    log::warn!("GPT ModelResponse - Failed to parse {:?}: {:#?}", why, line);
                }
            }
        }
        match final_response {
            Some(r) => {
                log::info!("GPT ModelResponse: {:#?}", r);
                r
            }
            None => {
                log::warn!("GPT ModelResponse - No done:true response in: {:#?}", json_string);
                panic!("Failed to parse.")
            }
        }
    };

    let rin_response = parse_rin_response(
        model_response
            .message
            .content
            .trim_end_matches("<｜end▁of▁sentence｜>")
            .split("</check>")
            .last()
            .unwrap()
            .trim(),
    );
    let now_ts = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let bot_msg = ModelMessageData {
        role: "assistant".to_string(),
        content: format!("[{}] {}", now_ts, rin_response.text),
        images: None,
    };
    let mut last_msg = map.messages.pop_back().unwrap(); // pop current user message (strip images before saving)
    last_msg.images = None; // We don't want to save the images, as it makes it take longer for subsequent gpt requests.
    map.messages.push_back(last_msg);
    map.messages.push_back(bot_msg);

    // Strip the RAG prefix before saving — it must not be persisted into AI_CONTEXT
    for _ in 0..rag_prefix_len {
        map.messages.pop_front();
    }
    log::info!("Length of Context: {}", map.messages.len());
    while map.messages.len() >= 10 {
        map.messages.pop_front();
    }
    let tx = db.begin_write()?;
    {
        let data = serde_json::to_string(&map.messages)?;
        let mut table = tx.open_table(AI_CONTEXT)?;
        let _ = table.insert(channel_id, data.as_str());
    }
    tx.commit()?;

    // Fire-and-forget memory save — does not block the response
    if crate::brain::is_enabled() {
        if let Some(mem) = &rin_response.memory {
            let title = mem.title.clone();
            let content = mem.content.clone();
            tokio::spawn(async move {
                crate::brain::retrieval::save_memory(&title, &content).await;
            });
        }
    }

    Ok(rin_response)
}

fn parse_rin_response(raw: &str) -> RinResponse {
    let clean = |mut r: RinResponse| -> RinResponse {
        r.text = r.text.trim().to_string();
        r
    };
    // Try direct parse
    if let Ok(r) = serde_json::from_str::<RinResponse>(raw) {
        return clean(r);
    }
    // Model may wrap JSON in markdown code fences
    if let Some(start) = raw.find('{') {
        if let Some(end) = raw.rfind('}') {
            if let Ok(r) = serde_json::from_str::<RinResponse>(&raw[start..=end]) {
                return clean(r);
            }
        }
    }
    log::warn!("Failed to parse structured response, falling back to plain text: {:?}", raw);
    RinResponse {
        text: raw.trim().to_string(),
        embed: None,
        react_user: None,
        react_self: None,
        memory: None,
    }
}
