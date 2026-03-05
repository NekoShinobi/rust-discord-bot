use std::path::PathBuf;

use crate::{Context, Error, colors};

use poise::serenity_prelude as serenity;
use poise::serenity_prelude::CreateAttachment;
use uuid::Uuid;
use yt_dlp::Downloader;
use yt_dlp::client::deps::Libraries;
use yt_dlp::VideoSelection;

async fn edit_audio_clip(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
    start_time: u32,
    duration: u32,
) -> Result<(), Error> {
    let start_time_str = format!("{}", start_time);
    let duration_str = format!("{}", duration);

    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-ss",
            &start_time_str,
            "-t",
            &duration_str,
            "-i",
            input_path.to_str().unwrap(),
            "-acodec",
            "mp3",
            "-y",
            output_path.to_str().unwrap(),
        ])
        .status()
        .await?;

    if !status.success() {
        return Err("FFmpeg command failed".into());
    }

    Ok(())
}

#[poise::command(prefix_command, slash_command, category = "Music")]
pub async fn yt_edit(
    ctx: Context<'_>,
    #[description = "YouTube URL of the video to clip from"] url: String,
    #[description = "Start time in seconds"] start_time: u32,
    #[description = "Duration of the clip in seconds (default 30)"] duration: Option<u32>,
) -> Result<(), Error> {
    let duration = duration.unwrap_or(30).min(30);

    let libraries_dir = PathBuf::from("/usr/bin");
    let output_dir = PathBuf::from("output");

    let youtube = libraries_dir.join("yt-dlp");
    let ffmpeg = libraries_dir.join("ffmpeg");
    let libraries = Libraries::new(youtube, ffmpeg);

    log::info!("Downloading and editing clip from URL: {}", url);

    let downloader = Downloader::builder(libraries, output_dir.clone())
        .build()
        .await?;

    log::info!("Initialized downloader.");

    let video = downloader.fetch_video_infos(url.clone()).await?;
    log::info!("Fetched video info: {:?}", video.title);
    log::debug!("Fetched video info: {:?}", video.extractor_info);

    let audio_format = video.best_audio_format().unwrap();

    log::info!("Using audio format: {:?}", audio_format);

    // Generate random filename to avoid conflicts
    let random_id = Uuid::new_v4();
    let audio_filename = format!("clip_{}.mp3", random_id);
    let clip_filename = format!("edited_clip_{}.mp3", random_id);

    log::info!("Downloading audio to temporary file: {}", audio_filename);

    // Download the best audio format
    let audio_path = output_dir.join(&audio_filename);
    downloader
        .download(&video, &audio_path)
        .execute_audio_stream()
        .await?;

    let clip_path = output_dir.join(&clip_filename);
    log::info!("Editing audio clip to file: {}", clip_path.display());
    edit_audio_clip(&audio_path, &clip_path, start_time, duration).await?;

    let reply = {
        let embed = serenity::CreateEmbed::new()
            .title(video.title.to_string())
            .description(format!(
                "Here is your audio clip from {} (start: {}s, duration: {}s)",
                video.title, start_time, duration
            ))
            .fields(vec![
                ("Original Video", format!("[Link]({})", url), false),
                ("Duration", format!("{} seconds", duration), true),
                ("Views", format!("{}", video.view_count.unwrap_or(0)), true),
                ("Likes", format!("{}", video.like_count.unwrap_or(0)), true),
                (
                    "Comments",
                    format!("{}", video.comment_count.unwrap_or(0)),
                    true,
                ),
            ])
            .url(url)
            .thumbnail(video.thumbnail.unwrap_or_default())
            .color(colors::PINK)
            .footer(serenity::CreateEmbedFooter::new(format!(
                "Requested by {}",
                ctx.author().name
            )));

        poise::CreateReply::default().embed(embed)
    };
    // Reply with the audio clip attached
    ctx.send(reply.attachment(CreateAttachment::path(&clip_path).await?))
        .await?;

    Ok(())
}
