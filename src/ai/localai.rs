// https://github.com/ollama/ollama/blob/main/docs/api.md

use std::collections::VecDeque;

use crate::env::LOCALAI_URL;
use crate::{AI_CONTEXT, Error, HTTP_CLIENT, KV_DATABASE};

use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Utc};
use poise::serenity_prelude as serenity;
use redb::ReadableDatabase;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
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
}

impl Default for OllamaOptions {
    fn default() -> Self {
        OllamaOptions {
            num_ctx: 4096,
            temperature: 0.7,
            num_predict: 10000,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelData {
    pub model: String,
    pub messages: VecDeque<ModelMessageData>,
    pub options: OllamaOptions,
    pub stream: bool,
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
    pub done_reason: String,
    pub done: bool,
    pub total_duration: u64,
    pub load_duration: u64,
    pub prompt_eval_count: u64,
    pub prompt_eval_duration: u64,
    pub eval_count: u64,
    pub eval_duration: u64,
}

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
) -> Result<String, Error> {
    let db = KV_DATABASE.get().unwrap();
    let rx = db.begin_read()?;
    let rx_table = rx.open_table(AI_CONTEXT)?;

    let mut map = ModelData::new();

    let channel_id: &str = &message.channel_id.to_string();

    if let Some(stored_value) = rx_table.get(channel_id)? {
        let mut stored_messages: VecDeque<ModelMessageData> =
            serde_json::from_str(stored_value.value())?;
        map.messages.append(&mut stored_messages);
    }

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

    let new_msg = ModelMessageData {
        role: "user".to_string(),
        content: format!(
            "{} says: {}",
            message.author.name,
            message
                .content_safe(&ctx.cache)
                .replace("@Rin#7236", "")
                .trim_start(),
        ),
        images: Some(images),
    };
    map.messages.push_back(new_msg);

    let system_message = ModelMessageData {
        role: "system".to_string(),
        content: system_prompt.to_string(),
        images: None,
    };
    map.messages.push_back(system_message);

    log::debug!("GPT Sent {:#?}", map.messages);
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!("{}/api/chat", &*LOCALAI_URL))
        .json(&map)
        .send()
        .await?;

    let json_string = resp.text().await?;
    // Deserialize the JSON string into a Value
    let results: Result<ModelResponse, serde_json::Error> =
        serde_json::from_str(json_string.as_str());

    let model_response = match results {
        Ok(response) => {
            log::info!("GPT ModelResponse: {:#?}", response);
            response
        }
        Err(why) => {
            log::warn!(
                "GPT ModelResponse - Failed to parse {:?}: {:#?}",
                why,
                json_string
            );
            panic!("Failed to parse.")
        }
    };

    // Deepseek has a think section, which should be removed
    let bot_msg = ModelMessageData {
        role: "assistant".to_string(),
        content: model_response.message.content.clone(),
        images: None,
    };
    map.messages.pop_back(); // Popping system message
    let mut last_msg = map.messages.pop_back().unwrap(); // popping user message
    last_msg.images = None; // We don't want to save the images, as it makes it take longer for subsequent gpt requests.
    map.messages.push_back(last_msg);
    map.messages.push_back(bot_msg);
    log::info!("Length of Context: {}", map.messages.len());
    while map.messages.len() >= 40 {
        map.messages.pop_front();
    }
    let content = model_response
        .message
        .content
        .trim_end_matches("<｜end▁of▁sentence｜>")
        .split("</check>")
        .last()
        .unwrap()
        .to_string();

    let tx = db.begin_write()?;
    {
        let data = serde_json::to_string(&map.messages)?;
        let mut table = tx.open_table(AI_CONTEXT)?;
        let _ = table.insert(channel_id, data.as_str());
    }
    tx.commit()?;

    Ok(content)
}
