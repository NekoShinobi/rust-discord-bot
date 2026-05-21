use std::sync::LazyLock;

pub static AUTHOR_ID: LazyLock<u64> =
    LazyLock::new(|| std::env::var("AUTHOR_ID").unwrap().parse::<u64>().unwrap());

pub static DISCORD_TOKEN: LazyLock<String> =
    LazyLock::new(|| std::env::var("DISCORD_TOKEN").unwrap());

pub static FOOTER_URL: LazyLock<String> = LazyLock::new(|| std::env::var("FOOTER_URL").unwrap());

pub static LOCALAI_URL: LazyLock<String> = LazyLock::new(|| std::env::var("LOCALAI_URL").unwrap());

pub static SERVE_STATIC_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("SERVE_STATIC_URL").unwrap());

pub static SHOKO_SERVER_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("SHOKO_SERVER_URL").unwrap());

pub static SHOKO_SERVER_API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("SHOKO_SERVER_API_KEY").unwrap());

pub static SONARR_URL: LazyLock<String> = LazyLock::new(|| std::env::var("SONARR_URL").unwrap());

pub static SONARR_API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("SONARR_API_KEY").unwrap());

pub static OPENWEATHERMAP_API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("OPENWEATHERMAP_API_KEY").unwrap());

pub static KICK_CLIENT_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("KICK_CLIENT_ID").unwrap());

pub static KICK_CLIENT_SECRET: LazyLock<String> =
    LazyLock::new(|| std::env::var("KICK_CLIENT_SECRET").unwrap());

pub static TMDB_API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("TMDB_API_KEY").unwrap());

pub static QDRANT_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("QDRANT_URL").unwrap());

pub static EMBEDDING_MODEL: LazyLock<String> =
    LazyLock::new(|| std::env::var("EMBEDDING_MODEL").unwrap_or_else(|_| "nomic-embed-text".to_string()));

pub static EMBEDDING_DIM: LazyLock<u64> =
    LazyLock::new(|| std::env::var("EMBEDDING_DIM").ok().and_then(|v| v.parse().ok()).unwrap_or(768));

pub static QUERY_EXPAND_MODEL: LazyLock<String> =
    LazyLock::new(|| std::env::var("QUERY_EXPAND_MODEL").unwrap_or_else(|_| "gemma4:e2b".to_string()));

pub static OUTLINE_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("OUTLINE_URL").unwrap_or_default());

pub static OUTLINE_API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("OUTLINE_API_KEY").unwrap_or_default());

pub static OUTLINE_COLLECTION_ID: LazyLock<String> =
    LazyLock::new(|| std::env::var("OUTLINE_COLLECTION_ID").unwrap_or_default());
