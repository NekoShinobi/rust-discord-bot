use crate::colors;
use crate::{AYDY, Context, Error, KV_DATABASE};
use chrono::Timelike;
use poise::serenity_prelude as serenity;
use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, interval};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AydyState {
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    pub message_id: Option<u64>,
    pub last_sent: i64,                           // Unix timestamp
    #[serde(default)]
    pub last_scheduled_sent: i64,                 // Unix timestamp of last *scheduled* send (not force)
    pub enrolled_users: HashMap<u64, UserStatus>, // user_id -> UserStatus
    #[serde(default = "default_send_hour")]
    pub send_hour: u32,                           // UTC hour to send the daily check (0-23)
    #[serde(default)]
    pub send_minute: u32,                         // UTC minute to send the daily check (0-59)
}

fn default_send_hour() -> u32 { 12 } // Default: 12:00 UTC

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserStatus {
    pub user_name: String,
    pub last_response: i64,     // Unix timestamp
    pub enrolled_at: i64,       // Unix timestamp
    #[serde(default)]
    pub death_count: u32,       // How many times this user has died
    #[serde(default)]
    pub last_alive_start: i64,  // When the current alive streak started
    #[serde(default)]
    pub is_currently_dead: bool, // Whether the user is currently in a "dead" state
}

impl UserStatus {
    /// Get the current alive streak in seconds
    pub fn current_alive_streak(&self) -> i64 {
        let alive_start = if self.last_alive_start == 0 {
            // For legacy users without last_alive_start, use Feb 1, 2026 as migration date
            // This prevents showing streaks from Unix epoch (1970)
            1769904000 // Feb 1, 2026 00:00:00 UTC
        } else {
            self.last_alive_start
        };
        self.last_response - alive_start
    }

    /// Check if the user is considered "dead" (no response in 48 hours)
    pub fn is_dead(&self, hours: i64) -> bool {
        let cutoff = chrono::Utc::now().timestamp() - (hours * 3600);
        self.last_response < cutoff
    }

    /// Get the current alive streak in days
    pub fn alive_days(&self) -> f64 {
        self.current_alive_streak() as f64 / 86400.0
    }
}

impl AydyState {
    pub fn new(channel_id: u64, guild_id: Option<u64>) -> Self {
        let now = chrono::Utc::now();
        Self {
            channel_id,
            guild_id,
            message_id: None,
            last_sent: now.timestamp(),
            last_scheduled_sent: 0,
            enrolled_users: HashMap::new(),
            send_hour: now.hour(),
            send_minute: now.minute(),
        }
    }

    pub fn get_non_responders(&self, hours: i64) -> Vec<(u64, &UserStatus)> {
        let cutoff = chrono::Utc::now().timestamp() - (hours * 3600);
        self.enrolled_users
            .iter()
            .filter(|(_, status)| status.last_response < cutoff)
            .map(|(id, status)| (*id, status))
            .collect()
    }

    pub fn update_user_response(&mut self, user_id: u64, user_name: String) -> bool {
        let now = chrono::Utc::now().timestamp();
        let mut was_dead = false;

        if let Some(status) = self.enrolled_users.get_mut(&user_id) {
            // Check if user was dead and is coming back.
            // OR both conditions: the stored flag may be stale (false) if the hourly
            // checker hasn't run yet, so also check the 48-hour time threshold.
            was_dead = status.is_currently_dead || status.is_dead(48);
            if was_dead {
                // User is coming back from the dead, reset their alive streak
                status.last_alive_start = now;
                status.is_currently_dead = false;
            }
            status.last_response = now;
            status.user_name = user_name; // Update name in case it changed
        } else {
            // First time user is responding
            self.enrolled_users.insert(
                user_id,
                UserStatus {
                    user_name,
                    last_response: now,
                    enrolled_at: now,
                    death_count: 0,
                    last_alive_start: now,
                    is_currently_dead: false,
                },
            );
        }
        was_dead
    }

    /// Enroll a user if they are not already in the list (does not update existing users)
    pub fn enroll_user(&mut self, user_id: u64, user_name: String) {
        if !self.enrolled_users.contains_key(&user_id) {
            let now = chrono::Utc::now().timestamp();
            self.enrolled_users.insert(
                user_id,
                UserStatus {
                    user_name,
                    last_response: now,
                    enrolled_at: now,
                    death_count: 0,
                    last_alive_start: now,
                    is_currently_dead: false,
                },
            );
        }
    }

    /// Check for users who have crossed the 48-hour death threshold and update their status
    pub fn update_death_states(&mut self) -> bool {
        let mut state_changed = false;

        for status in self.enrolled_users.values_mut() {
            let is_dead_now = status.is_dead(48);

            // If user just became dead (wasn't dead before, but is now)
            if is_dead_now && !status.is_currently_dead {
                status.death_count += 1;
                status.is_currently_dead = true;
                state_changed = true;
            }
        }

        state_changed
    }
}

/// Start the "Are you dead yet?" check
#[poise::command(
    prefix_command,
    slash_command,
    subcommands("start", "stop", "status", "leaderboard", "schedule", "force"),
    category = "AYDY"
)]
pub async fn aydy(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Start the AYDY check in this channel
#[poise::command(prefix_command, slash_command)]
async fn start(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();
    let guild_id = ctx.guild_id().map(|g| g.get());

    // Check if AYDY is already running in this channel
    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    {
        let tx = db.begin_read()?;
        let table = tx.open_table(AYDY)?;
        if table.get(key.as_str())?.is_some() {
            let embed = serenity::CreateEmbed::new()
                .title("❌ Already Running")
                .description(
                    "AYDY is already running in this channel. Use `/aydy stop` to stop it first.",
                )
                .color(colors::ERROR)
                .timestamp(serenity::Timestamp::now());
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
            return Ok(());
        }
    }

    // Create new AYDY state
    let mut state = AydyState::new(channel_id, guild_id);

    // Auto-enroll all non-bot members who can see this channel
    sync_channel_members(&ctx.serenity_context().http, &mut state).await;

    // Send initial message
    let message = send_aydy_message(&ctx.serenity_context().http, &state).await?;

    // Update state with message ID and mark the scheduled send so the checker
    // doesn't immediately fire again — use the canonical scheduled time, not now,
    // so the next send lands at H:M tomorrow.
    state.message_id = Some(message.id.get());
    state.last_scheduled_sent = most_recent_scheduled_occurrence(state.send_hour, state.send_minute).timestamp();

    // Save to database
    {
        let tx = db.begin_write()?;
        {
            let mut table = tx.open_table(AYDY)?;
            let value = serde_json::to_string(&state)?;
            table.insert(key.as_str(), value.as_str())?;
        }
        tx.commit()?;
    }

    let embed = serenity::CreateEmbed::new()
        .title("✅ AYDY Check Started")
        .description("I'll send a message every 24 hours. Users can click the button to check in!")
        .color(colors::SUCCESS)
        .timestamp(serenity::Timestamp::now());
    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// Stop the AYDY check in this channel
#[poise::command(prefix_command, slash_command)]
async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();
    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    let tx = db.begin_write()?;
    {
        let mut table = tx.open_table(AYDY)?;
        if table.remove(key.as_str())?.is_none() {
            drop(table);
            drop(tx);
            let embed = serenity::CreateEmbed::new()
                .title("❌ Not Running")
                .description("AYDY is not running in this channel.")
                .color(colors::ERROR)
                .timestamp(serenity::Timestamp::now());
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
            return Ok(());
        }
    }
    tx.commit()?;

    let embed = serenity::CreateEmbed::new()
        .title("✅ AYDY Check Stopped")
        .description("The AYDY check has been stopped in this channel.")
        .color(colors::SUCCESS)
        .timestamp(serenity::Timestamp::now());
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Check the current AYDY status
#[poise::command(prefix_command, slash_command)]
async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();
    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    let tx = db.begin_read()?;
    let table = tx.open_table(AYDY)?;

    if let Some(state_str) = table.get(key.as_str())? {
        let state_json: &str = state_str.value();
        let state: AydyState = serde_json::from_str(state_json)?;

        let mut description = String::new();
        description.push_str(&format!(
            "**Enrolled users:** {}\n",
            state.enrolled_users.len()
        ));

        let non_responders = state.get_non_responders(48);
        description.push_str(&format!(
            "**Non-responders (48h):** {}\n\n",
            non_responders.len()
        ));

        if !state.enrolled_users.is_empty() {
            description.push_str("**Enrolled Users:**\n");
            for user_status in state.enrolled_users.values() {
                description.push_str(&format!(
                    "• {} - Last response: <t:{}:R>\n",
                    user_status.user_name, user_status.last_response
                ));
            }
        }

        let embed = serenity::CreateEmbed::new()
            .title("📊 AYDY Status")
            .description(description)
            .color(colors::INFO)
            .timestamp(serenity::Timestamp::now());
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    } else {
        let embed = serenity::CreateEmbed::new()
            .title("❌ Not Running")
            .description("AYDY is not running in this channel.")
            .color(colors::ERROR)
            .timestamp(serenity::Timestamp::now());
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    }

    Ok(())
}

/// Set the daily time (UTC) at which the AYDY check message is sent
#[poise::command(prefix_command, slash_command)]
async fn schedule(
    ctx: Context<'_>,
    #[description = "Hour to send the daily check (UTC, 0-23)"] hour: u32,
    #[description = "Minute to send the daily check (0-59, default 0)"] minute: Option<u32>,
) -> Result<(), Error> {
    let minute = minute.unwrap_or(0);
    if hour > 23 || minute > 59 {
        let embed = serenity::CreateEmbed::new()
            .title("❌ Invalid Time")
            .description("Hour must be 0–23 and minute must be 0–59.")
            .color(colors::ERROR)
            .timestamp(serenity::Timestamp::now());
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
        return Ok(());
    }

    let channel_id = ctx.channel_id().get();
    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    let tx = db.begin_write()?;
    let committed = {
        let mut table = tx.open_table(AYDY)?;
        let existing = table.get(key.as_str())?.map(|v| v.value().to_string());
        if let Some(state_str) = existing {
            let mut state: AydyState = serde_json::from_str(&state_str)?;
            state.send_hour = hour;
            state.send_minute = minute;
            let value = serde_json::to_string(&state)?;
            table.insert(key.as_str(), value.as_str())?;
            true
        } else {
            false
        }
    };
    tx.commit()?;

    if !committed {
        let embed = serenity::CreateEmbed::new()
            .title("❌ Not Running")
            .description("AYDY is not running in this channel.")
            .color(colors::ERROR)
            .timestamp(serenity::Timestamp::now());
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
        return Ok(());
    }

    let embed = serenity::CreateEmbed::new()
        .title("✅ Schedule Updated")
        .description(format!(
            "AYDY check will now be sent daily at **{:02}:{:02} UTC**.",
            hour, minute
        ))
        .color(colors::SUCCESS)
        .timestamp(serenity::Timestamp::now());
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Immediately send the AYDY check message regardless of schedule
#[poise::command(prefix_command, slash_command)]
async fn force(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();
    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    let mut state: AydyState = {
        let tx = db.begin_read()?;
        let table = tx.open_table(AYDY)?;
        if let Some(state_str) = table.get(key.as_str())? {
            serde_json::from_str(state_str.value())?
        } else {
            let embed = serenity::CreateEmbed::new()
                .title("❌ Not Running")
                .description("AYDY is not running in this channel.")
                .color(colors::ERROR)
                .timestamp(serenity::Timestamp::now());
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
            return Ok(());
        }
    };

    let message = send_aydy_message(&ctx.serenity_context().http, &state).await?;
    state.message_id = Some(message.id.get());
    state.last_sent = chrono::Utc::now().timestamp();

    {
        let tx = db.begin_write()?;
        {
            let mut table = tx.open_table(AYDY)?;
            let value = serde_json::to_string(&state)?;
            table.insert(key.as_str(), value.as_str())?;
        }
        tx.commit()?;
    }

    let embed = serenity::CreateEmbed::new()
        .title("✅ AYDY Sent")
        .description("AYDY check message sent immediately.")
        .color(colors::SUCCESS)
        .timestamp(serenity::Timestamp::now());
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Show the AYDY leaderboard
#[poise::command(prefix_command, slash_command, aliases("lb"))]
async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id().get();
    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    let tx = db.begin_read()?;
    let table = tx.open_table(AYDY)?;

    if let Some(state_str) = table.get(key.as_str())? {
        let state_json: &str = state_str.value();
        let state: AydyState = serde_json::from_str(state_json)?;

        if state.enrolled_users.is_empty() {
            let embed = serenity::CreateEmbed::new()
                .title("📊 AYDY Leaderboard")
                .description("No enrolled users yet!")
                .color(colors::INFO)
                .timestamp(serenity::Timestamp::now());
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
            return Ok(());
        }

        // Sort users by alive streak (descending), dead users go to the bottom
        let mut sorted_users: Vec<(u64, &UserStatus)> = state
            .enrolled_users
            .iter()
            .map(|(id, status)| (*id, status))
            .collect();
        sorted_users.sort_by(|a, b| {
            let a_dead = a.1.is_currently_dead || a.1.is_dead(48);
            let b_dead = b.1.is_currently_dead || b.1.is_dead(48);
            match (a_dead, b_dead) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => b.1.current_alive_streak().cmp(&a.1.current_alive_streak()),
            }
        });

        let medal = ["🥇", "🥈", "🥉"];
        let mut description = String::new();

        for (i, (_, user_status)) in sorted_users.iter().take(10).enumerate() {
            let is_dead = user_status.is_currently_dead || user_status.is_dead(48);
            let alive_days = if is_dead { 0.0 } else { user_status.alive_days() };

            let rank = medal.get(i).copied().unwrap_or("▫️");
            let status_icon = if is_dead { "<a:xqcL:923554657773699082>" } else { "<:xdd:1106775701832609902>" };

            let alive_str = if is_dead {
                "⚰️ R.I.P.".to_string()
            } else if alive_days < 1.0 {
                format!("{:.0}h", alive_days * 24.0)
            } else {
                format!("{:.1}d", alive_days)
            };

            let alive_label = if is_dead { "" } else { " alive" };
            description.push_str(&format!(
                "{} **{}** {} - {}{} · {} death{}\n",
                rank,
                user_status.user_name,
                status_icon,
                alive_str,
                alive_label,
                user_status.death_count,
                if user_status.death_count == 1 { "" } else { "s" },
            ));
        }

        let alive_count = sorted_users.iter().filter(|(_, s)| !s.is_currently_dead && !s.is_dead(48)).count();
        let dead_count = sorted_users.len() - alive_count;

        let embed = serenity::CreateEmbed::new()
            .title("💀 AYDY Leaderboard 💀")
            .description(description)
            .color(colors::INFO)
            .footer(serenity::CreateEmbedFooter::new(format!(
                "{} alive · {} dead · {} total enrolled",
                alive_count, dead_count, state.enrolled_users.len()
            )))
            .timestamp(serenity::Timestamp::now());
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    } else {
        let embed = serenity::CreateEmbed::new()
            .title("❌ Not Running")
            .description("AYDY is not running in this channel.")
            .color(colors::ERROR)
            .timestamp(serenity::Timestamp::now());
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    }

    Ok(())
}

async fn send_aydy_message(
    http: &Arc<serenity::Http>,
    state: &AydyState,
) -> Result<serenity::Message, Error> {
    let channel_id = serenity::ChannelId::new(state.channel_id);

    let embed = create_aydy_embed(state);

    let button = serenity::CreateButton::new("aydy_check")
        .label("Are you dead yet?")
        .style(serenity::ButtonStyle::Primary);

    let components = vec![serenity::CreateActionRow::Buttons(vec![button])];

    let builder = serenity::CreateMessage::new()
        .embed(embed)
        .components(components);

    let message = channel_id.send_message(http, builder).await?;

    Ok(message)
}

async fn update_aydy_message(http: &Arc<serenity::Http>, state: &AydyState) -> Result<(), Error> {
    if let Some(message_id) = state.message_id {
        let channel_id = serenity::ChannelId::new(state.channel_id);
        let message_id = serenity::MessageId::new(message_id);

        let embed = create_aydy_embed(state);

        let button = serenity::CreateButton::new("aydy_check")
            .label("Are you dead yet?")
            .style(serenity::ButtonStyle::Primary);

        let components = vec![serenity::CreateActionRow::Buttons(vec![button])];

        let builder = serenity::EditMessage::new()
            .embed(embed)
            .components(components);

        channel_id.edit_message(http, message_id, builder).await?;
    }

    Ok(())
}

fn create_aydy_embed(state: &AydyState) -> serenity::CreateEmbed {
    let mut description = String::from("Click the button below to let us know you're alive!\n\n");

    // Add enrolled users list
    if !state.enrolled_users.is_empty() {
        description.push_str("**Enrolled Users:**\n");
        for user_status in state.enrolled_users.values() {
            description.push_str(&format!("• {}\n", user_status.user_name));
        }
        description.push('\n');
    }

    // Add non-responders (48 hours)
    let non_responders = state.get_non_responders(48);
    description.push_str("**⚠️ No response in 48 hours:**\n");
    if non_responders.is_empty() {
        description.push_str("• None\n");
    } else {
        for (_, user_status) in &non_responders {
            description.push_str(&format!(
                "• {} (last seen: <t:{}:R>)\n",
                user_status.user_name, user_status.last_response
            ));
        }
    }

    serenity::CreateEmbed::new()
        .title("🩺 Are you dead yet?")
        .description(description)
        .color(colors::INFO)
        .timestamp(serenity::Timestamp::now())
}

pub async fn handle_aydy_button(
    ctx: &serenity::Context,
    interaction: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let channel_id = interaction.channel_id.get();
    let user_id = interaction.user.id.get();
    let user_name = interaction.user.name.clone();

    let db = KV_DATABASE.get().unwrap();
    let key = format!("aydy_{}", channel_id);

    // Load state
    let mut state: AydyState = {
        let tx = db.begin_read()?;
        let table: redb::ReadOnlyTable<&str, &str> = tx.open_table(AYDY)?;

        if let Some(state_str) = table.get(key.as_str())? {
            let state_json: &str = state_str.value();
            serde_json::from_str(state_json)?
        } else {
            // AYDY not running in this channel
            interaction
                .create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("❌ AYDY is not running in this channel anymore.")
                            .ephemeral(true),
                    ),
                )
                .await?;
            return Ok(());
        }
    };

    // Update user response
    let is_new = !state.enrolled_users.contains_key(&user_id);
    let was_dead = state.update_user_response(user_id, user_name.clone());

    // Save updated state
    {
        let tx = db.begin_write()?;
        {
            let mut table = tx.open_table(AYDY)?;
            let value = serde_json::to_string(&state)?;
            table.insert(key.as_str(), value.as_str())?;
        }
        tx.commit()?;
    }

    // Update the message
    update_aydy_message(&ctx.http, &state).await?;

    // Respond to the interaction
    let response_msg = if is_new {
        format!(
            "✅ Welcome {}! You've been enrolled in the AYDY check.",
            user_name
        )
    } else if was_dead {
        format!("💀 Welcome back from the dead, {}! Your death count is now {}.", user_name, state.enrolled_users.get(&user_id).map(|s| s.death_count).unwrap_or(0))
    } else {
        format!("✅ Thanks for checking in, {}!", user_name)
    };

    interaction
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content(response_msg)
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

/// Returns the most recent past wall-clock occurrence of H:M UTC.
/// If today's H:M hasn't arrived yet, returns yesterday's H:M.
fn most_recent_scheduled_occurrence(hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    let today = now.date_naive()
        .and_hms_opt(hour, minute, 0)
        .unwrap()
        .and_utc();
    if now >= today { today } else { today - chrono::Duration::hours(24) }
}

pub async fn start_aydy_checker(http: Arc<serenity::Http>) {
    let mut minute_interval = interval(Duration::from_secs(60));
    let mut hour_interval = interval(Duration::from_secs(3600));

    log::info!("AYDY checker started");

    loop {
        tokio::select! {
            _ = minute_interval.tick() => {
                if let Err(e) = check_and_send_aydy_messages(&http, false).await {
                    log::error!("Error in AYDY checker: {:?}", e);
                }
            }
            _ = hour_interval.tick() => {
                if let Err(e) = check_and_send_aydy_messages(&http, true).await {
                    log::error!("Error in AYDY hourly sync: {:?}", e);
                }
            }
        }
    }
}

async fn check_and_send_aydy_messages(http: &Arc<serenity::Http>, sync_members: bool) -> Result<(), Error> {
    let db = KV_DATABASE.get().unwrap();
    let tx = db.begin_read()?;
    let table = tx.open_table(AYDY)?;

    let mut updates = Vec::new();

    // Collect all AYDY states that need updates
    for entry in table.iter()? {
        let (key, value): (redb::AccessGuard<&str>, redb::AccessGuard<&str>) = entry?;
        let state_json: &str = value.value();
        let mut state: AydyState = serde_json::from_str(state_json)?;

        let now = chrono::Utc::now();
        let now_ts = now.timestamp();
        let mut needs_update = false;

        // Sync members and death states hourly only (avoids hammering Discord API every minute)
        if sync_members {
            if sync_channel_members(http, &mut state).await {
                needs_update = true;
            }
            if state.update_death_states() {
                needs_update = true;
            }
        }

        // Send if the most recent scheduled occurrence has not been sent yet.
        // Uses last_scheduled_sent (not last_sent) so force-sends don't suppress the schedule.
        // If the bot was offline during the scheduled window, this will fire immediately on
        // the next checker tick (catch-up). last_scheduled_sent is set to the *scheduled* time,
        // not the actual send time, so subsequent messages still land at the original H:M.
        let due = most_recent_scheduled_occurrence(state.send_hour, state.send_minute);
        if state.last_scheduled_sent < due.timestamp() {
            if let Ok(message) = send_aydy_message(http, &state).await {
                state.message_id = Some(message.id.get());
                state.last_sent = now_ts;
                state.last_scheduled_sent = due.timestamp();
                needs_update = true;
            }
        }

        if needs_update {
            updates.push((key.value().to_string(), state));
        }
    }

    drop(table);
    drop(tx);

    // Save updates
    if !updates.is_empty() {
        let tx = db.begin_write()?;
        {
            let mut table = tx.open_table(AYDY)?;
            for (key, state) in updates {
                let value = serde_json::to_string(&state)?;
                table.insert(key.as_str(), value.as_str())?;
            }
        }
        tx.commit()?;
    }

    Ok(())
}

/// Fetches all non-bot members who have access to the AYDY channel and enrolls any that
/// are not yet in the state. Returns true if any new members were added.
async fn sync_channel_members(http: &Arc<serenity::Http>, state: &mut AydyState) -> bool {
    let guild_id = match state.guild_id {
        Some(id) => serenity::GuildId::new(id),
        None => return false,
    };
    let channel_id = serenity::ChannelId::new(state.channel_id);

    // Fetch the channel to get its permission overwrites
    let channel = match http.get_channel(channel_id).await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to fetch channel for AYDY sync: {:?}", e);
            return false;
        }
    };
    let guild_channel = match channel.guild() {
        Some(gc) => gc,
        None => return false,
    };

    // Fetch the guild to get its roles (needed for permission calculation)
    let guild = match http.get_guild(guild_id).await {
        Ok(g) => g,
        Err(e) => {
            log::warn!("Failed to fetch guild for AYDY sync: {:?}", e);
            return false;
        }
    };

    // Fetch all guild members
    let members = match http.get_guild_members(guild_id, Some(1000), None).await {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Failed to fetch guild members for AYDY sync: {:?}", e);
            return false;
        }
    };

    let before = state.enrolled_users.len();
    for member in members {
        if member.user.bot {
            continue;
        }
        // Calculate the member's effective permissions in the channel
        let perms = guild.user_permissions_in(&guild_channel, &member);
        if perms.view_channel() {
            state.enroll_user(member.user.id.get(), member.user.name.clone());
        }
    }
    state.enrolled_users.len() > before
}

/// Helper function to truncate a string to a specified length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
