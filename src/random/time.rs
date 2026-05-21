use crate::colors;
use crate::{Context, Error};
use chrono::Utc;
use chrono_tz::Tz;
use poise::serenity_prelude as serenity;

/// Display the current time in multiple timezones.
#[poise::command(prefix_command, slash_command, category = "Basic")]
pub async fn time(ctx: Context<'_>) -> Result<(), Error> {
    let now = Utc::now();

    let zones: &[(&str, Tz)] = &[
        ("🇺🇸 Los Angeles", chrono_tz::America::Los_Angeles),
        ("🗽 New York", chrono_tz::America::New_York),
        ("🌐 UTC", chrono_tz::UTC),
        ("🇯🇵 Japan", chrono_tz::Japan),
    ];

    let mut description = String::new();
    for (label, tz) in zones {
        let local = now.with_timezone(tz);
        description.push_str(&format!(
            "**{}**\n{}\n\n",
            label,
            local.format("%A, %B %-d %Y  %I:%M:%S %p %Z")
        ));
    }

    let embed = serenity::CreateEmbed::new()
        .title("Current Time")
        .description(description.trim_end())
        .color(colors::ROSEWATER)
        .timestamp(serenity::model::Timestamp::now());

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
