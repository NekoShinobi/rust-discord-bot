use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use crate::colors;
use crate::env::{FOOTER_URL, SHOKO_SERVER_API_KEY, SHOKO_SERVER_URL};
use crate::{Context, Error, HTTP_CLIENT};

use poise::serenity_prelude as serenity;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use strsim::jaro_winkler;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ExpressionType {
    r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Expression {
    left: ExpressionType,
    parameter: String,
    r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShokoSeriesRequest {
    apply_at_series_level: bool,
    expression: Expression,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Link {
    r#type: String,
    name: String,
    u_r_l: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Poster {
    i_d: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Image {
    posters: Vec<Poster>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IDs {
    ani_d_b: u32,
    m_a_l: Vec<u32>,
    i_d: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IDWithoutMAL {
    ani_d_b: u32,
    i_d: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ReleaseGroup {
    i_d: u32,
    name: Option<String>,
    short_name: Option<String>,
    source: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AniDBEpisode {
    release_group: ReleaseGroup,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AniDBRating {
    value: Option<f32>,
    max_value: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AniDB {
    description: String,
    air_date: String,
    end_date: String,
    episode_count: u32,
    rating: Option<AniDBRating>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Episode {
    i_ds: IDWithoutMAL,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaInfo {
    bit_rate: u32,
    video: Vec<Video>,
    audio: Vec<Audio>,
    subtitles: Vec<Subtitle>,
    chapters: Vec<Chapter>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Video {
    resolution: String,
    bit_rate: u32,
    bit_depth: u8,
    codec: Codec,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Audio {
    channels: u8,
    channel_layout: Option<String>,
    samples_per_frame: u32,
    sampling_rate: u32,
    compression_mode: String,
    bit_rate: u32,
    codec: Codec,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Subtitle {
    title: Option<String>,
    order: u32,
    language: String,
    codec: Codec,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Chapter {
    title: String,
    language: String,
    timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Codec {
    simplified: String,
    raw: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct File {
    i_d: u32,
    size: u64,
    ani_d_b: Option<AniDBEpisode>,
    media_info: Option<MediaInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FileSource {
    unknown: u32,
    other: u32,
    t_v: u32,
    d_v_d: u32,
    blu_ray: u32,
    web: u32,
    v_h_s: u32,
    v_c_d: u32,
    laser_disc: u32,
    camera: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Sizes {
    file_sources: FileSource,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Anime {
    name: String,
    images: Image,
    i_ds: IDs,
    size: u32,
    links: Vec<Link>,
    ani_d_b: Option<AniDB>,
    sizes: Sizes,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShokoFileResponse {
    total: u32,
    list: Vec<File>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShokoEpisodeResponse {
    total: u32,
    list: Vec<Episode>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShokoSeriesResponse {
    list: Vec<Anime>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShokoStats {
    file_count: u32,
    series_count: u32,
    group_count: u32,
    file_size: u64,
    finished_series: u32,
    watched_episodes: u32,
    watched_hours: f64,
    percent_duplicate: f64,
    missing_episodes: u32,
    missing_episodes_collecting: u32,
    unrecognized_files: u32,
    series_with_missing_links: u32,
    episodes_with_multiple_files: u32,
    files_with_duplicate_locations: u32,
}

// Jikan (MyAnimeList) API structures
#[derive(Debug, Serialize, Deserialize)]
struct JikanAnimeData {
    mal_id: u32,
    title: String,
    title_english: Option<String>,
    synopsis: Option<String>,
    episodes: Option<u32>,
    score: Option<f32>,
    images: JikanImages,
    aired: JikanAired,
}

#[derive(Debug, Serialize, Deserialize)]
struct JikanImages {
    jpg: JikanImageUrls,
}

#[derive(Debug, Serialize, Deserialize)]
struct JikanImageUrls {
    large_image_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JikanAired {
    from: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JikanResponse {
    data: Vec<JikanAnimeData>,
}

fn format_links(text: &str) -> String {
    let pattern = r"(http://anidb.net/.*?) \[([^\]]+)\]"; // Regex pattern
    let re = Regex::new(pattern).unwrap();
    re.replace_all(text, |m: &Captures| {
        format!(
            "[{}]({})",
            m.get(2).unwrap().as_str(),
            m.get(1).unwrap().as_str()
        )
    })
    .to_string()
}

fn find_best_scoring_string<'a>(text: &String, results: &'a ShokoSeriesResponse) -> &'a Anime {
    let mut best_score = 0.0;
    let mut best_match: &Anime = &results.list[0];

    for anime in &results.list {
        let score = jaro_winkler(text, anime.name.as_str());
        if score > best_score {
            best_score = score;
            best_match = anime;
        }
    }

    best_match
}

fn format_bytes(bytes: u64) -> String {
    const TB: u64 = 1_099_511_627_776; // 1024^4
    const GB: u64 = 1_073_741_824; // 1024^3
    const MB: u64 = 1_048_576; // 1024^2
    const KB: u64 = 1_024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

static ANIME_ID_MAP: OnceLock<HashMap<u32, AnimeIds>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SeasonInfo {
    tvdb: Option<u32>,
    tmdb: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnimeIds {
    #[serde(rename = "type")]
    anime_type: Option<String>,
    anidb_id: Option<u32>,
    anilist_id: Option<u32>,
    animecountdown_id: Option<u32>,
    animenewsnetwork_id: Option<u32>,
    #[serde(rename = "anime-planet_id")]
    anime_planet_id: Option<String>,
    anisearch_id: Option<u32>,
    imdb_id: Option<String>,
    kitsu_id: Option<u32>,
    livechart_id: Option<u32>,
    mal_id: Option<u32>,
    simkl_id: Option<u32>,
    themoviedb_id: Option<u32>,
    tvdb_id: Option<u32>,
    season: Option<SeasonInfo>,
}

async fn fetch_and_cache_anime_list() -> Result<HashMap<u32, AnimeIds>, Box<dyn std::error::Error>> {
    let url = "https://raw.githubusercontent.com/Fribb/anime-lists/refs/heads/master/anime-list-full.json";
    let local_path = "anime-list-full.json";
    let update_interval = Duration::from_secs(60 * 60 * 24); // 24 hours

    let needs_update = match fs::metadata(local_path) {
        Ok(meta) => match meta.modified() {
            Ok(modified) => {
                SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or(update_interval + Duration::from_secs(1))
                    > update_interval
            }
            Err(_) => true,
        },
        Err(_) => true,
    };

    if needs_update {
        log::info!("Fetching updated anime list from GitHub...");
        let resp = reqwest::get(url).await?;
        let remote_bytes = resp.bytes().await?;
        fs::write(local_path, &remote_bytes)?;
        log::info!("Anime list updated successfully");
    }

    let data = fs::read(local_path)?;
    let anime_list: Vec<AnimeIds> = serde_json::from_slice(&data)?;

    // Build HashMap with anidb_id as key
    let mut map = HashMap::new();
    for anime in anime_list {
        if anime.anidb_id.is_some() {
            map.insert(anime.anidb_id.unwrap(), anime);
        }
    }

    log::info!("Loaded {} anime entries into map", map.len());
    Ok(map)
}

fn get_anime_ids(anidb_id: u32) -> Option<AnimeIds> {
    let result = ANIME_ID_MAP.get().and_then(|map| map.get(&anidb_id).cloned());
    if let Some(ref ids) = result {
        log::info!("Found anime IDs for anidb_id {}: anilist={:?}, mal={:?}, tmdb={:?}, imdb={:?}",
            anidb_id, ids.anilist_id, ids.mal_id, ids.themoviedb_id, ids.imdb_id);
    } else {
        log::warn!("No anime IDs found for anidb_id {}", anidb_id);
    }
    result
}

async fn initialize_anime_map() {
    if ANIME_ID_MAP.get().is_none() {
        match fetch_and_cache_anime_list().await {
            Ok(map) => {
                let _ = ANIME_ID_MAP.set(map);
            }
            Err(e) => {
                log::error!("Failed to initialize anime ID map: {}", e);
            }
        }
    }
}

fn format_cross_platform_links(ids: &AnimeIds) -> String {
    let mut links = Vec::new();

    if let Some(anidb_id) = ids.anidb_id {
        log::debug!("Formatting links for anidb_id {}: anilist={:?}, mal={:?}, animeplanet={:?}, imdb={:?}, simkl={:?}, tmdb={:?}, tvdb={:?}",
            anidb_id, ids.anilist_id, ids.mal_id, ids.anime_planet_id, ids.imdb_id, ids.simkl_id, ids.themoviedb_id, ids.tvdb_id);

        links.push(format!("[AniDB](https://anidb.net/anime/{})", anidb_id));
    }

    if let Some(anilist) = ids.anilist_id {
        links.push(format!("[AniList](https://anilist.co/anime/{})", anilist));
    }

    if let Some(animeplanet) = &ids.anime_planet_id {
        links.push(format!("[Anime-Planet](https://www.anime-planet.com/anime/{})", animeplanet));
    }

    if let Some(imdb) = &ids.imdb_id {
        links.push(format!("[IMDb](https://www.imdb.com/title/{})", imdb));
    }

    if let Some(mal) = ids.mal_id {
        links.push(format!("[MyAnimeList](https://myanimelist.net/anime/{})", mal));
    }

    if let Some(simkl) = ids.simkl_id {
        links.push(format!("[SIMKL](https://simkl.com/anime/{})", simkl));
    }

    if let Some(tmdb) = ids.themoviedb_id {
        // Use type from the anime data, default to "tv"
        let tmdb_type = ids.anime_type.as_deref().unwrap_or("TV");
        let tmdb_path = if tmdb_type == "Movie" || tmdb_type == "movie" { "movie" } else { "tv" };
        links.push(format!("[TMDB](https://www.themoviedb.org/{}/{})", tmdb_path, tmdb));
    }

    if let Some(tvdb) = ids.tvdb_id {
        links.push(format!("[TVDB](https://thetvdb.com/?tab=series&id={})", tvdb));
    }

    if links.is_empty() {
        "No external links available".to_string()
    } else {
        links.join(" • ")
    }
}

fn convert_anidb_rating_to_10(rating: &AniDBRating) -> Option<f32> {
    rating.value.map(|value| (value / rating.max_value as f32) * 10.0)
}

async fn search_mal_fallback(query: &str) -> Result<Option<JikanAnimeData>, Error> {
    log::info!("Searching MAL via Jikan API for: {}", query);

    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .get(format!("https://api.jikan.moe/v4/anime?q={}&limit=1",
            urlencoding::encode(query)))
        .send()
        .await?;

    let text = resp.text().await?;
    log::info!("Jikan API returned: {}", text);

    let jikan_resp: JikanResponse = serde_json::from_str(&text)?;
    Ok(jikan_resp.data.into_iter().next())
}

#[poise::command(
    prefix_command,
    slash_command,
    subcommands("search", "stats"),
    subcommand_required,
    category = "Anime"
)]
pub async fn shoko(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(prefix_command, slash_command, category = "Anime")]
pub async fn stats(ctx: Context<'_>) -> Result<(), Error> {
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .get(format!("{}/api/v3/Dashboard/Stats", &*SHOKO_SERVER_URL))
        .header("apikey", &*SHOKO_SERVER_API_KEY)
        .send()
        .await?;

    let text = resp.text().await?;
    log::info!("Shoko Server returned {}", text.as_str());

    let stats: ShokoStats = serde_json::from_str(text.as_str())?;

    let footer = serenity::CreateEmbedFooter::new(format!("Powered by {}", &*FOOTER_URL));
    let reply = {
        let embed = serenity::CreateEmbed::new()
            .title("VFS Anime Stats")
            .fields(vec![
                ("Files Count", format!("{}", &stats.file_count), true),
                ("Series Count", format!("{}", &stats.series_count), true),
                ("File Size", format_bytes(stats.file_size), true),
            ])
            .footer(footer)
            // Add a timestamp for the current time
            // This also accepts a rfc3339 Timestamp
            .timestamp(serenity::model::Timestamp::now());

        poise::CreateReply::default().embed(embed)
    };

    ctx.send(reply).await?;
    Ok(())
}

#[poise::command(prefix_command, slash_command, category = "Anime")]
pub async fn search(ctx: Context<'_>, value: String) -> Result<(), Error> {
    // Initialize anime map if not already done
    initialize_anime_map().await;

    let map = ShokoSeriesRequest {
        apply_at_series_level: false,
        expression: Expression {
            left: ExpressionType {
                r#type: "NamesSelector".to_string(),
            },
            parameter: value,
            r#type: "AnyContains".to_string(),
        },
    };
    log::info!("Making HTTP Request to Shoko Server with AnyContains");
    let resp = HTTP_CLIENT
        .get()
        .unwrap()
        .post(format!(
            "{}/api/v3/Filter/Preview/Series?pageSize=100&page=1",
            &*SHOKO_SERVER_URL
        ))
        .header("apikey", &*SHOKO_SERVER_API_KEY)
        .json(&map)
        .send()
        .await?;

    let text = resp.text().await?;
    log::info!("Shoko Server returned {}", text.as_str());
    let results: ShokoSeriesResponse = serde_json::from_str(text.as_str())?;

    // Check if Shoko found results, otherwise fall back to MAL
    if results.list.is_empty() {
        log::warn!("No Shoko results found for search: {}, trying MAL fallback", map.expression.parameter);

        match search_mal_fallback(&map.expression.parameter).await? {
            Some(mal_anime) => {
                return handle_mal_result(ctx, mal_anime).await;
            }
            None => {
                ctx.send(poise::CreateReply::default().content("No results found in Shoko or MyAnimeList."))
                    .await?;
                return Ok(());
            }
        }
    }

    let result = find_best_scoring_string(&map.expression.parameter, &results);

    log::info!("Fetching poster, series details, and episodes in parallel");

    // Start all three API calls in parallel
    let poster_future = async {
        HTTP_CLIENT
            .get()
            .unwrap()
            .get(format!(
                "{}/api/v3/Image/AniDB/Poster/{}",
                &*SHOKO_SERVER_URL, &result.images.posters[0].i_d
            ))
            .send()
            .await
    };

    let series_future = async {
        let resp = HTTP_CLIENT
            .get()
            .unwrap()
            .get(format!(
                "{}/api/v3/Series/{}?includeDataFrom=AniDB",
                &*SHOKO_SERVER_URL, &result.i_ds.i_d
            ))
            .header("apikey", &*SHOKO_SERVER_API_KEY)
            .send()
            .await?;
        let text = resp.text().await?;
        log::info!("Shoko Server returned series data");
        let series: Anime = serde_json::from_str(text.as_str())?;
        Ok::<Anime, Error>(series)
    };

    let episodes_future = async {
        let resp = HTTP_CLIENT
            .get()
            .unwrap()
            .get(format!(
                "{}/api/v3/Series/{}/Episode?pageSize=1&includeMissing=false&includeWatched=true&includeHidden=false&includeUnaired=false",
                &*SHOKO_SERVER_URL, &result.i_ds.i_d
            ))
            .header("apikey", &*SHOKO_SERVER_API_KEY)
            .send()
            .await?;
        let text = resp.text().await?;
        log::info!("Shoko Server returned episode data");
        let episodes: ShokoEpisodeResponse = serde_json::from_str(text.as_str())?;
        Ok::<ShokoEpisodeResponse, Error>(episodes)
    };

    // Await poster first to send initial embed quickly
    let poster = poster_future.await?;
    let attachment: serenity::CreateAttachment =
        serenity::CreateAttachment::bytes(poster.bytes().await.unwrap(), "poster.png");

    // Send initial embed with basic info
    let initial_embed = serenity::CreateEmbed::new()
        .title(&result.name)
        .description("Loading anime details...")
        .image("attachment://poster.png")
        .color(colors::PRIMARY);

    let initial_reply = poise::CreateReply::default()
        .embed(initial_embed)
        .attachment(attachment.clone());
    let reply_handle = ctx.send(initial_reply).await?;

    // Now await the remaining API calls
    let (series_result, episodes_result) = tokio::join!(series_future, episodes_future);
    let series: Anime = series_result?;
    let episodes: ShokoEpisodeResponse = episodes_result?;

    let anidb = &series.ani_d_b.unwrap();

    // Get cross-platform IDs from cached anime list
    let cross_platform_ids = get_anime_ids(result.i_ds.ani_d_b);
    let platform_links = cross_platform_ids
        .as_ref()
        .map(|ids| format_cross_platform_links(ids))
        .unwrap_or_else(|| format!("[AniDB](https://anidb.net/anime/{})", result.i_ds.ani_d_b));

    // Get AniList ID for releases.moe lookup
    let alid = cross_platform_ids
        .as_ref()
        .and_then(|ids| ids.anilist_id)
        .unwrap_or(0);

    let mut release_group: String = "Unknown Release Group".to_string();
    let mut video_codec: String = "Unknown Video Codec".to_string();
    let mut audio_codec: String = "Unknown Audio Codec".to_string();
    let mut video_bit_depth: u8 = 10;
    let mut video_resolution: String = "Unknown Resolution".to_string();
    let mut video_bit_rate: f32 = 0.0;

    if episodes.total == 0 {
        log::warn!("No episodes found for series {}", result.name);
    } else {
        // Grab the File so we can figure out the release group.
        let resp = HTTP_CLIENT
            .get()
            .unwrap()
            .get(format!(
                "{}/api/v3/Episode/{}/File?includeDataFrom=AniDB&include=MediaInfo",
                &*SHOKO_SERVER_URL, &episodes.list[0].i_ds.i_d
            ))
            .header("apikey", &*SHOKO_SERVER_API_KEY)
            .send()
            .await?;
        let text = resp.text().await?;
        log::info!("Shoko Server returned {}", text.as_str());
        let files: ShokoFileResponse = serde_json::from_str(text.as_str())?;

        if let Some(file) = files.list.first() {
            let media_info = file.media_info.as_ref().unwrap();
            video_codec = media_info.video.first().unwrap().codec.simplified.clone();
            audio_codec = media_info.audio.first().unwrap().codec.simplified.clone();
            video_bit_depth = media_info.video.first().unwrap().bit_depth;
            video_resolution = media_info.video.first().unwrap().resolution.clone();
            video_bit_rate = media_info.bit_rate as f32 / 1024.0 / 1024.0; // Convert to Mb/s

            if let Some(ani_d_b) = &file.ani_d_b {
                release_group = ani_d_b.release_group.name.clone().unwrap_or(release_group);
            } else {
                log::warn!("No AniDB data found for file ID {}", file.i_d);
            }
        } else {
            log::warn!("No files found for series {}", result.name);
            release_group = "No Files Found".to_string();
        }
    }

    // Info from SeaDex if this release is a good one.
    log::info!("anidb_id {} - alid {}", result.i_ds.ani_d_b, alid);
    let mut group_list = "No releases found".to_string();
    let mut comparisons = "No comparisons found".to_string();
    if alid != 0 {
        let resp = HTTP_CLIENT
            .get()
            .unwrap()
            .get(format!(
                "https://releases.moe/api/collections/entries/records?filter=alID={}&expand=trs&filter=trs.tracker=%27Nyaa%27",
                &alid,
            ))
            .send()
            .await?;
        let text = resp.text().await?;
        log::info!("Releases.moe returned {}", text.as_str());

        // Extract unique release group names
        let mut release_groups = String::new();
        let mut comps = String::new();
        let json: serde_json::Value = serde_json::from_str(&text)?;
        if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
            if items.is_empty() {
                log::info!("No releases.moe entries found for alid {}", alid);
            } else {
                // There should only be 1 item.
                let item = items.first().unwrap();

                // Grab "comparison" and split by string and enumerate
                item.get("comparison")
                    .and_then(|c| c.as_str())
                    .unwrap()
                    .split(',')
                    .enumerate()
                    .for_each(|(i, comp)| {
                        comps.push_str(format!("[{}]({}) ", i + 1, comp).as_str());
                    });

                if let Some(trs) = item
                    .get("expand")
                    .and_then(|expand| expand.get("trs"))
                    .and_then(|trs| trs.as_array())
                {
                    for tr in trs {
                        // Check if tracker value is == "Nyaa"
                        if let Some(tracker) = tr.get("tracker").and_then(|t| t.as_str())
                            && tracker != "Nyaa"
                        {
                            continue;
                        }
                        if let Some(group) = tr.get("releaseGroup").and_then(|g| g.as_str()) {
                            release_groups.push_str(
                                format!(
                                    "[{}]({})\n",
                                    group,
                                    tr.get("url").and_then(|u| u.as_str()).unwrap_or("")
                                )
                                .as_str(),
                            );
                        }
                    }
                }
            }
        }

        // If we found any release groups, use them
        if !release_groups.is_empty() {
            group_list = release_groups;
        }

        // If we found any release groups, use them
        if !comps.is_empty() {
            comparisons = comps;
        }
    }

    // Collect file sources with count > 0
    let mut file_sources = Vec::new();
    let fs = &series.sizes.file_sources;

    if fs.unknown > 0 {
        file_sources.push("Unknown");
    }
    if fs.other > 0 {
        file_sources.push("Other");
    }
    if fs.t_v > 0 {
        file_sources.push("TV");
    }
    if fs.d_v_d > 0 {
        file_sources.push("DVD");
    }
    if fs.blu_ray > 0 {
        file_sources.push("BD");
    }
    if fs.web > 0 {
        file_sources.push("Web");
    }
    if fs.v_h_s > 0 {
        file_sources.push("VHS");
    }
    if fs.v_c_d > 0 {
        file_sources.push("VCD");
    }
    if fs.laser_disc > 0 {
        file_sources.push("LaserDisc");
    }
    if fs.camera > 0 {
        file_sources.push("Camera");
    }

    let file_sources_text = if file_sources.is_empty() {
        "None".to_string()
    } else {
        file_sources.join(" | ")
    };

    let footer = serenity::CreateEmbedFooter::new(format!("Powered by {}", &*FOOTER_URL));

    // Build description with formatted AniDB description and external links
    let description = format!(
        "{}\n\n**External Links:**\n{}",
        format_links(&anidb.description),
        platform_links
    );

    let reply = {
        let embed = serenity::CreateEmbed::new()
            .title(&result.name)
            .url(&result.links[0].u_r_l)
            .description(description)
            .image("attachment://poster.png")
            .color(colors::SKY)
            .fields(vec![
                ("Episodes", format!("{}", &anidb.episode_count), true),
                ("Aired Date", anidb.air_date.to_string(), true),
                (
                    "Score",
                    anidb.rating.as_ref()
                        .and_then(|r| convert_anidb_rating_to_10(r))
                        .map(|score| format!("{:.2}/10 (AniDB)", score))
                        .unwrap_or_else(|| "N/A".to_string()),
                    true,
                ),
                (
                    "Releases.moe",
                    if alid > 0 {
                        format!("[releases.moe](https://releases.moe/{})\n{}", alid, group_list)
                    } else {
                        "N/A".to_string()
                    },
                    true,
                ),
                (
                    "Current Shoko Release",
                    format!("[{}]({}/webui/collection/series/{}/overview)", release_group, &*SHOKO_SERVER_URL, result.i_ds.i_d),
                    true,
                ),
                (
                    "File Details",
                    format!("[{}][{}][{}-bit][{}][{:.2} Mb/s]", video_codec, audio_codec, video_bit_depth, video_resolution, video_bit_rate),
                    true,
                ),
                (
                    "File Sources",
                    file_sources_text,
                    true,
                ),
                (
                    "Comparisons",
                    comparisons,
                    false,
                )
            ])
            .footer(footer)
            // Add a timestamp for the current time
            // This also accepts a rfc3339 Timestamp
            .timestamp(serenity::model::Timestamp::now());

        poise::CreateReply::default().embed(embed)
    };

    reply_handle.edit(ctx, reply).await?;
    Ok(())
}

async fn handle_mal_result(ctx: Context<'_>, mal_anime: JikanAnimeData) -> Result<(), Error> {
    log::info!("Handling MAL result for: {}", mal_anime.title);

    // Get cross-platform IDs if we have a MAL ID
    initialize_anime_map().await;
    let cross_platform_ids = ANIME_ID_MAP.get()
        .and_then(|map| {
            map.values()
                .find(|anime| anime.mal_id == Some(mal_anime.mal_id))
                .cloned()
        });

    let platform_links = cross_platform_ids
        .as_ref()
        .map(|ids| format_cross_platform_links(ids))
        .unwrap_or_else(|| format!("[MyAnimeList](https://myanimelist.net/anime/{})", mal_anime.mal_id));

    let _alid = cross_platform_ids
        .as_ref()
        .and_then(|ids| ids.anilist_id)
        .unwrap_or(0);

    // Format description
    let synopsis = mal_anime.synopsis.unwrap_or_else(|| "No synopsis available.".to_string());
    let description = format!(
        "{}\n\n**External Links:**\n{}",
        synopsis,
        platform_links
    );

    // Format aired date
    let aired_date = mal_anime.aired.from
        .and_then(|date_str| {
            chrono::DateTime::parse_from_rfc3339(&date_str).ok()
        })
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let footer = serenity::CreateEmbedFooter::new(format!("Powered by {}", &*FOOTER_URL));

    let mut embed = serenity::CreateEmbed::new()
        .title(&mal_anime.title)
        .url(format!("https://myanimelist.net/anime/{}", mal_anime.mal_id))
        .description(description)
        .color(colors::SKY)
        .fields(vec![
            ("Episodes", mal_anime.episodes.map(|e| e.to_string()).unwrap_or_else(|| "Unknown".to_string()), true),
            ("Aired Date", aired_date, true),
            (
                "Score",
                mal_anime.score
                    .map(|score| format!("{:.2}/10 (MAL)", score))
                    .unwrap_or_else(|| "N/A".to_string()),
                true,
            ),
        ])
        .footer(footer)
        .timestamp(serenity::model::Timestamp::now());

    // Use direct image URL instead of downloading
    if let Some(image_url) = mal_anime.images.jpg.large_image_url {
        embed = embed.image(image_url);
    }

    let reply = poise::CreateReply::default().embed(embed);

    ctx.send(reply).await?;
    Ok(())
}
