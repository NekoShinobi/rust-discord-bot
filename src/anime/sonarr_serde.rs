use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Show {
    pub title: String,
    pub alternate_titles: Vec<AlternateTitle>,
    pub sort_title: String,
    pub status: String,
    pub ended: bool,
    pub overview: String,
    pub previous_airing: Option<DateTime<Utc>>,
    pub next_airing: Option<DateTime<Utc>>,
    pub network: String,
    pub air_time: String,
    pub images: Vec<Image>,
    pub original_language: Language,
    pub seasons: Vec<Season>,
    pub year: u16,
    pub path: String,
    pub quality_profile_id: u16,
    pub season_folder: bool,
    pub monitored: bool,
    pub monitor_new_items: String,
    pub use_scene_numbering: bool,
    pub runtime: u16,
    pub tvdb_id: u32,
    pub tv_rage_id: u32,
    pub tv_maze_id: u32,
    pub first_aired: DateTime<Utc>,
    pub last_aired: DateTime<Utc>,
    pub series_type: String,
    pub clean_title: String,
    pub imdb_id: Option<String>,
    pub title_slug: String,
    pub root_folder_path: String,
    pub certification: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<u32>,
    pub added: DateTime<Utc>,
    pub ratings: Ratings,
    pub statistics: Statistics,
    pub language_profile_id: u16,
    pub id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternateTitle {
    pub title: String,
    pub scene_season_number: Option<isize>,
    pub season_number: Option<isize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub cover_type: String,
    pub url: String,
    pub remote_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Season {
    pub season_number: u16,
    pub monitored: bool,
    pub statistics: SeasonStatistics,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonStatistics {
    pub next_airing: Option<DateTime<Utc>>,
    pub previous_airing: Option<DateTime<Utc>>,
    pub episode_file_count: u32,
    pub episode_count: u32,
    pub total_episode_count: u32,
    pub size_on_disk: u64,
    pub release_groups: Vec<String>,
    pub percent_of_episodes: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ratings {
    pub votes: u32,
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub season_count: u16,
    pub episode_file_count: u32,
    pub episode_count: u32,
    pub total_episode_count: u32,
    pub size_on_disk: u64,
    pub release_groups: Vec<String>,
    pub percent_of_episodes: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub episode_id: u32,
    pub series_id: u32,
    pub source_title: String,
    pub languages: Vec<Language>,
    pub quality: Quality,
    pub custom_formats: Vec<CustomFormat>,
    pub custom_format_score: u16,
    pub quality_cutoff_not_met: bool,
    pub date: DateTime<Utc>,
    pub download_id: String,
    pub event_type: String,
    pub data: HistoryData,
    pub id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub quality: QualityObject,
    pub revision: RevisionObject,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityObject {
    pub id: u32,
    pub name: String,
    pub source: String,
    pub resolution: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionObject {
    pub version: u32,
    pub real: u32,
    pub is_repack: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFormat {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryData {
    pub indexer: String,
    pub nzb_info_url: String,
    pub release_group: String,
    pub age: String,
    pub age_hours: String,
    pub age_minutes: String,
    pub published_date: DateTime<Utc>,
    pub download_client: String,
    pub download_client_name: String,
    pub size: String,
    pub download_url: String,
    pub guid: String,
    pub tvdb_id: String,
    pub tv_rage_id: String,
    pub protocol: String,
    pub custom_format_score: String,
    pub series_match_type: String,
    pub release_source: String,
    pub indexer_flags: String,
    pub release_type: String,
    pub torrent_info_hash: String,
}
