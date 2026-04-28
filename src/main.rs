mod ai;
mod anime;
mod colors;
mod config;
mod db;
mod env;
mod fixembed;
mod language;
mod music;
mod permissions;
mod random;
mod streams;
mod tickets;
mod tmdb;
mod utility;
mod web;

use std::sync::OnceLock;
use std::time::Instant;

use miette::Result;
use poise::serenity_prelude as serenity;
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, LevelFilter, TermLogger, TerminalMode, WriteLogger,
};
use songbird::SerenityInit;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
// User data, which is stored and accessible in all command invocations
#[derive(Debug)]
pub struct Data {}

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static KV_DATABASE: OnceLock<redb::Database> = OnceLock::new();
static REACTION_CONFIG: OnceLock<config::Config> = OnceLock::new();
static START_TIME: OnceLock<Instant> = OnceLock::new();
static COMMANDS_LIST: OnceLock<Vec<serde_json::Value>> = OnceLock::new();
static EMOJIS_LIST: OnceLock<Vec<serde_json::Value>> = OnceLock::new();
static DISCORD_HTTP: OnceLock<std::sync::Arc<serenity::Http>> = OnceLock::new();

const TABLE: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("tags");
const AI_CONTEXT: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("context");
const STREAMS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("streams");
const LIVE_STREAMS_STATE: redb::TableDefinition<&str, &str> =
    redb::TableDefinition::new("live_streams_state");
const PERMISSIONS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("permissions");
const HISTORY: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("history");
const AYDY: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("aydy");
const PAPERS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("papers");
const TICKETS: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("tickets");
const ACTIVE_TICKETS: redb::TableDefinition<&str, &str> =
    redb::TableDefinition::new("active_tickets");

fn replace_usernames_with_mentions(response: &str, guild: &serenity::Guild) -> String {
    use regex::Regex;
    let mut result = response.to_string();
    for (user_id, member) in &guild.members {
        let mention = format!("<@{}>", user_id);
        let mut names: Vec<&str> = vec![member.user.name.as_str()];
        if let Some(nick) = &member.nick {
            names.push(nick.as_str());
        }
        if let Some(global) = &member.user.global_name {
            names.push(global.as_str());
        }
        names.dedup();
        for name in names {
            if name.len() < 3 {
                continue;
            }
            let escaped = regex::escape(name);
            let pattern = format!(r"(?i)\b{}\b", escaped);
            if let Ok(re) = Regex::new(&pattern) {
                result = re.replace_all(&result, mention.as_str()).to_string();
            }
        }
    }
    result
}

fn split_string_chunks(long_string: &str, chunk_size: usize) -> Vec<String> {
    long_string
        .chars()
        .collect::<Vec<char>>() // Convert to Vec<char> to use chunks()
        .chunks(chunk_size)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

// Event Handler
async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            START_TIME.get_or_init(Instant::now);
            log::info!("Logged in as {}", data_about_bot.user.name);

            // Collect emojis from all guilds
            let mut emojis = Vec::new();
            for guild_id in ctx.cache.guilds() {
                if let Some(guild) = ctx.cache.guild(guild_id) {
                    for emoji in guild.emojis.values() {
                        emojis.push(serde_json::json!({
                            "id": emoji.id.to_string(),
                            "name": emoji.name,
                            "animated": emoji.animated,
                            "url": emoji.url(),
                            "guild_id": guild_id.to_string(),
                            "guild_name": guild.name.clone(),
                        }));
                    }
                }
            }
            log::info!(
                "Collected {} emojis from {} guilds",
                emojis.len(),
                ctx.cache.guilds().len()
            );
            let _ = EMOJIS_LIST.set(emojis);
        }
        serenity::FullEvent::Message { new_message } => 'message_match: {
            let lower_case_msg = new_message.content.to_lowercase();
            if new_message.author.id == ctx.cache.current_user().id {
                break 'message_match;
            }
            if new_message.mentions.contains(&**ctx.cache.current_user()) {
                if lower_case_msg.contains("please wipe all context")
                    && new_message.author.id == *env::AUTHOR_ID
                {
                    ai::localai::wipe_context(new_message).await?;
                    new_message
                        .reply(ctx, "Wiped all context.".to_string())
                        .await?;
                } else {
                    let start = Instant::now();
                    let typing = ctx.http.start_typing(new_message.channel_id);

                    let config = REACTION_CONFIG.get().unwrap();
                    let system_prompt = config
                        .ai
                        .as_ref()
                        .map(|ai| ai.system_prompt.as_str())
                        .unwrap_or("You are a helpful AI assistant.");

                    let response =
                        ai::localai::get_gpt_response(new_message, ctx, system_prompt).await?;

                    let response = if let Some(guild_id) = new_message.guild_id {
                        if let Some(guild) = ctx.cache.guild(guild_id) {
                            replace_usernames_with_mentions(&response, &guild)
                        } else {
                            response
                        }
                    } else {
                        response
                    };

                    log::info!("Response: {:#?}", response);
                    let chunks = split_string_chunks(&response, 2000);
                    log::info!("Chunks: {:#?}", chunks);
                    for chunk in chunks.iter() {
                        new_message.reply(ctx, chunk.to_string()).await?;
                    }

                    typing.stop();
                    let duration = start.elapsed();
                    println!("Time elapsed for GPT Response is: {:?}", duration);
                }
            }

            let config = REACTION_CONFIG.get().unwrap();
            random::response::handle_message_response(ctx, new_message, config).await?;

            fixembed::process_fix_embed(ctx, new_message).await?;
        }
        serenity::FullEvent::InteractionCreate { interaction } => {
            if let serenity::Interaction::Component(component_interaction) = interaction {
                if component_interaction.data.custom_id == "aydy_check" {
                    log::info!(
                        "{:?} clicked the AYDY check button",
                        component_interaction.user
                    );
                    if let Err(e) =
                        random::deadge::handle_aydy_button(ctx, component_interaction).await
                    {
                        log::error!("Error handling AYDY button: {:?}", e);
                    }
                } else if component_interaction
                    .data
                    .custom_id
                    .starts_with("papers_role_")
                {
                    log::info!(
                        "{:?} clicked a papers role button",
                        component_interaction.user
                    );
                    if let Err(e) =
                        utility::papers::handle_papers_button(ctx, component_interaction).await
                    {
                        log::error!("Error handling papers button: {:?}", e);
                    }
                } else if component_interaction
                    .data
                    .custom_id
                    .starts_with("ticket_create_")
                {
                    log::info!(
                        "{:?} clicked a ticket creation button",
                        component_interaction.user
                    );
                    if let Err(e) = tickets::handle_ticket_button(ctx, component_interaction).await
                    {
                        log::error!("Error handling ticket button: {:?}", e);
                    }
                } else if component_interaction
                    .data
                    .custom_id
                    .starts_with("ticket_delete_")
                    || component_interaction
                        .data
                        .custom_id
                        .starts_with("ticket_close_confirm_")
                    || component_interaction
                        .data
                        .custom_id
                        .starts_with("ticket_close_cancel_")
                {
                    log::info!(
                        "{:?} clicked a ticket management button",
                        component_interaction.user
                    );
                    if let Err(e) =
                        tickets::handle_ticket_delete_button(ctx, component_interaction).await
                    {
                        log::error!("Error handling ticket management button: {:?}", e);
                    }
                }
            } else if let serenity::Interaction::Modal(modal_interaction) = interaction
                && modal_interaction
                    .data
                    .custom_id
                    .starts_with("ticket_modal_")
            {
                log::info!("{:?} submitted a ticket modal", modal_interaction.user);
                if let Err(e) = tickets::handle_ticket_modal(ctx, modal_interaction).await {
                    log::error!("Error handling ticket modal: {:?}", e);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

async fn discordbot(subsys: &mut tokio_graceful_shutdown::SubsystemHandle) -> Result<()> {
    log::info!("Initializing clients and databases...");

    KV_DATABASE.get_or_init(|| {
        let mut db = redb::Database::create("storage.db").unwrap();

        // Make sure tables exist
        {
            let tx = db.begin_write().unwrap();
            tx.open_table(TABLE).unwrap();
            tx.open_table(AI_CONTEXT).unwrap();
            tx.open_table(STREAMS).unwrap();
            tx.open_table(PERMISSIONS).unwrap();
            tx.open_table(HISTORY).unwrap();
            tx.open_table(AYDY).unwrap();
            tx.open_table(LIVE_STREAMS_STATE).unwrap();
            tx.open_table(PAPERS).unwrap();
            tx.open_table(TICKETS).unwrap();
            tx.open_table(ACTIVE_TICKETS).unwrap();
            tx.commit().unwrap();
        }
        db.compact().unwrap();
        db
    });

    HTTP_CLIENT.get_or_init(reqwest::Client::new);
    REACTION_CONFIG.get_or_init(|| config::load_config().unwrap());

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                ai::sd::stablediffusion(),
                anime::op::guess(),
                anime::shoko::shoko(),
                anime::sonarr::sonarr(),
                random::basic::avatar(),
                random::basic::bing(),
                random::basic::ping(),
                random::basic::help(),
                random::basic::uptime(),
                random::basic::userinfo(),
                random::basic::guildinfo(),
                random::time::time(),
                random::deadge::aydy(),
                random::fox::fox(),
                random::weather::weather(),
                language::chinese::hanzi(),
                language::kanji::kanji(),
                music::play::music(),
                music::musicclip::yt_edit(),
                streams::follow(),
                streams::unfollow(),
                streams::following(),
                streams::preview(),
                permissions::addperm(),
                permissions::removeperm(),
                permissions::listperms(),
                utility::tags::tag(),
                utility::papers::papers(),
                utility::profile::setstatus(),
                utility::profile::setavatar(),
                utility::profile::setbanner(),
                utility::profile::setactivity(),
                tickets::close(),
                tickets::force_close(),
                tmdb::movie(),
                tmdb::tv(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                ..Default::default()
            },
            on_error: |error| {
                Box::pin(async move {
                    match error {
                        poise::FrameworkError::Command { error, ctx, .. } => {
                            log::error!("Command error in '{}': {:?}", ctx.command().name, error);
                            let error_msg =
                                format!("❌ Error: {}\nDetails:\n```\n{:?}\n```", error, error);
                            let _ = ctx.say(error_msg).await;

                            // Log failed command to history
                            if let Some(db) = KV_DATABASE.get() {
                                let timestamp = chrono::Utc::now().to_rfc3339();
                                let user = ctx.author();
                                let command_name = ctx.command().qualified_name.clone();

                                // Capture full invocation with arguments
                                let full_invocation = match ctx {
                                    poise::Context::Prefix(pctx) => pctx.msg.content.clone(),
                                    poise::Context::Application(actx) => {
                                        // For slash commands, reconstruct from command name and options
                                        let mut invocation = command_name.to_string();
                                        let options = &actx.interaction.data.options;
                                        if !options.is_empty() {
                                            for option in options {
                                                invocation.push_str(&format!(
                                                    " {}:{:?}",
                                                    option.name, option.value
                                                ));
                                            }
                                        }
                                        invocation
                                    }
                                };

                                let (guild, channel) = match ctx {
                                    poise::Context::Prefix(pctx) => (
                                        pctx.msg
                                            .guild_id
                                            .map(|g| g.to_string())
                                            .unwrap_or_else(|| "DM".to_string()),
                                        pctx.msg.channel_id.to_string(),
                                    ),
                                    poise::Context::Application(actx) => (
                                        actx.interaction
                                            .guild_id
                                            .map(|g| g.to_string())
                                            .unwrap_or_else(|| "DM".to_string()),
                                        actx.interaction.channel_id.to_string(),
                                    ),
                                };

                                let history_entry = serde_json::json!({
                                    "timestamp": timestamp,
                                    "user": user.name,
                                    "user_id": user.id.to_string(),
                                    "command": command_name,
                                    "full_invocation": full_invocation,
                                    "guild": guild,
                                    "channel": channel,
                                    "success": false,
                                    "error": format!("{:?}", error)
                                });

                                let key = format!("{}_{}", timestamp, user.id);

                                if let Ok(tx) = db.begin_write() {
                                    {
                                        if let Ok(mut table) = tx.open_table(HISTORY) {
                                            let _ = table.insert(
                                                key.as_str(),
                                                history_entry.to_string().as_str(),
                                            );
                                        }
                                    }
                                    let _ = tx.commit();
                                }
                            }
                        }
                        poise::FrameworkError::ArgumentParse { error, .. } => {
                            if let Some(error) = error.downcast_ref::<serenity::RoleParseError>() {
                                log::warn!("Found a RoleParseError: {:?}", error);
                            } else {
                                log::warn!("ArgumentParse error: {}", error);
                            }
                        }
                        other => {
                            log::error!("Framework error occurred");
                            poise::builtins::on_error(other).await.unwrap()
                        }
                    }
                })
            },
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            post_command: |ctx| {
                Box::pin(async move {
                    // Log command execution to history
                    let db = match KV_DATABASE.get() {
                        Some(db) => db,
                        None => return,
                    };

                    let timestamp = chrono::Utc::now().to_rfc3339();
                    let user = ctx.author();
                    let command_name = ctx.command().qualified_name.clone();

                    // Capture full invocation with arguments
                    let full_invocation = match ctx {
                        poise::Context::Prefix(pctx) => pctx.msg.content.clone(),
                        poise::Context::Application(actx) => {
                            // For slash commands, reconstruct from command name and options
                            let mut invocation = format!("/{}", &command_name);
                            let options = &actx.interaction.data.options;
                            if !options.is_empty() {
                                for option in options {
                                    invocation
                                        .push_str(&format!(" {}:{:?}", option.name, option.value));
                                }
                            }
                            invocation
                        }
                    };

                    let (guild, channel) = match ctx {
                        poise::Context::Prefix(pctx) => (
                            pctx.msg
                                .guild_id
                                .map(|g| g.to_string())
                                .unwrap_or_else(|| "DM".to_string()),
                            pctx.msg.channel_id.to_string(),
                        ),
                        poise::Context::Application(actx) => (
                            actx.interaction
                                .guild_id
                                .map(|g| g.to_string())
                                .unwrap_or_else(|| "DM".to_string()),
                            actx.interaction.channel_id.to_string(),
                        ),
                    };

                    let history_entry = serde_json::json!({
                        "timestamp": timestamp,
                        "user": user.name,
                        "user_id": user.id.to_string(),
                        "command": command_name,
                        "full_invocation": full_invocation,
                        "guild": guild,
                        "channel": channel,
                        "success": true
                    });

                    let key = format!("{}_{}", timestamp, user.id);

                    if let Ok(tx) = db.begin_write() {
                        {
                            if let Ok(mut table) = tx.open_table(HISTORY) {
                                let _ =
                                    table.insert(key.as_str(), history_entry.to_string().as_str());
                            }
                        }
                        let _ = tx.commit();
                    }
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                // Extract command metadata for web API
                fn extract_commands(
                    cmd: &poise::Command<crate::Data, Box<dyn std::error::Error + Send + Sync>>,
                    parent_name: Option<&str>,
                ) -> Vec<serde_json::Value> {
                    let mut results = Vec::new();

                    // Skip parent command if it has subcommand_required
                    let has_subcommands = !cmd.subcommands.is_empty();
                    let subcommand_required = cmd.subcommand_required;

                    if !subcommand_required || !has_subcommands {
                        let category = cmd
                            .category
                            .as_ref()
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "Other".to_string());

                        // Build full command path
                        let full_command_path = if let Some(parent) = parent_name {
                            format!("{} {}", parent, cmd.name)
                        } else {
                            cmd.name.clone()
                        };

                        let usage = if !cmd.parameters.is_empty() {
                            let params: Vec<String> = cmd
                                .parameters
                                .iter()
                                .map(|p| {
                                    if p.required {
                                        format!("<{}>", p.name)
                                    } else {
                                        format!("[{}]", p.name)
                                    }
                                })
                                .collect();
                            format!("/{} {}", full_command_path, params.join(" "))
                        } else {
                            format!("/{}", full_command_path)
                        };

                        // Format command name with aliases (using full path)
                        let name_with_aliases = if !cmd.aliases.is_empty() {
                            format!("{} ({})", full_command_path, cmd.aliases.join(", "))
                        } else {
                            full_command_path.clone()
                        };

                        // Build full description with parameter details
                        let mut full_description = cmd
                            .description
                            .as_deref()
                            .unwrap_or("No description available")
                            .to_string();

                        if !cmd.parameters.is_empty() {
                            full_description.push_str("\n\n**Parameters:**");
                            for param in &cmd.parameters {
                                let param_desc =
                                    param.description.as_deref().unwrap_or("No description");
                                let required_marker = if param.required {
                                    "(required)"
                                } else {
                                    "(optional)"
                                };
                                full_description.push_str(&format!(
                                    "\n• `{}` {} - {}",
                                    param.name, required_marker, param_desc
                                ));
                            }
                        }

                        results.push(serde_json::json!({
                            "name": name_with_aliases,
                            "description": full_description,
                            "category": category,
                            "usage": usage
                        }));
                    }

                    // Recursively extract subcommands
                    if has_subcommands {
                        for subcmd in &cmd.subcommands {
                            let parent = if let Some(p) = parent_name {
                                format!("{} {}", p, cmd.name)
                            } else {
                                cmd.name.clone()
                            };
                            results.extend(extract_commands(subcmd, Some(&parent)));
                        }
                    }

                    results
                }

                let commands_metadata: Vec<serde_json::Value> = framework
                    .options()
                    .commands
                    .iter()
                    .flat_map(|cmd| extract_commands(cmd, None))
                    .collect();

                let _ = COMMANDS_LIST.set(commands_metadata);

                Ok(Data {})
            })
        })
        .build();

    let token = &*env::DISCORD_TOKEN;
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let activity = serenity::ActivityData {
        name: "Always Watching You~".to_string(),
        kind: serenity::ActivityType::Custom,
        state: None,
        url: None,
    };

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .activity(activity)
        .register_songbird()
        .await
        .expect("Error Creating Client");

    // Store HTTP client for web API
    DISCORD_HTTP.get_or_init(|| client.http.clone());

    // Start the stream checker background task
    let http_clone = client.http.clone();
    tokio::spawn(async move {
        streams::start_stream_checker(http_clone).await;
    });

    // Start the AYDY checker background task
    let http_clone2 = client.http.clone();
    tokio::spawn(async move {
        random::deadge::start_aydy_checker(http_clone2).await;
    });

    log::info!("Starting Discord Client...");
    let _ = client
        .start()
        .await
        .map_err(|why| println!("Client ended: {:?}", why));
    subsys.on_shutdown_requested().await;
    log::info!("Shutting down!");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider before any async operations
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let config = ConfigBuilder::new()
        .add_filter_ignore_str("tracing")
        .add_filter_ignore_str("serenity")
        .build();

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            config,
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("bot.log")
                .unwrap(),
        ),
    ])
    .unwrap();
    log::info!("Starting tokio startup...");

    // Setup and execute subsystem tree
    Toplevel::new(async |s: &mut SubsystemHandle| {
        s.start(SubsystemBuilder::new("DiscordBot", discordbot));
        s.start(SubsystemBuilder::new("WebServer", web::start_web_server));
    })
    .catch_signals()
    .handle_shutdown_requests(std::time::Duration::from_millis(1000))
    .await
    .map_err(Into::into)
}
