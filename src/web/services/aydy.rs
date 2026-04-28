use actix_web::{HttpResponse, Responder, get};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct LeaderboardEntry {
    user_name: String,
    alive_days: f64,
    death_count: u32,
    last_response: i64,
    enrolled_at: i64,
}

#[get("/api/aydy")]
pub async fn get_aydy() -> impl Responder {
    match crate::db::read_table(crate::AYDY, |key, value| {
        // Parse the JSON value to extract AYDY state
        serde_json::from_str::<serde_json::Value>(value)
            .ok()
            .and_then(|state| {
                let mut state_obj = state.as_object().cloned()?;
                state_obj.insert("channel_key".to_string(), serde_json::json!(key));

                // Enrich enrolled_users with computed leaderboard data
                if let Some(enrolled_users) = state_obj.get_mut("enrolled_users") {
                    if let Some(users_obj) = enrolled_users.as_object_mut() {
                        for (_user_id, user_data) in users_obj.iter_mut() {
                            if let Some(user_obj) = user_data.as_object_mut() {
                                // Calculate alive_days
                                let last_alive_start = match user_obj.get("last_alive_start") {
                                    Some(v) => {
                                        let timestamp = v.as_i64().unwrap_or(0);
                                        if timestamp == 0 { 1769904000 } else { timestamp }
                                    },
                                    None => 1769904000 // Feb 1, 2026 00:00:00 UTC for legacy users
                                };

                                if let Some(last_response) = user_obj.get("last_response").and_then(|v| v.as_i64()) {
                                    // A user is dead if the stored flag says so OR they haven't responded in 48h.
                                    // We must OR both conditions because is_currently_dead may be stale/false
                                    // for legacy users whose state was never updated by the bot timer.
                                    let stored_dead = user_obj.get("is_currently_dead")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    let now = chrono::Utc::now().timestamp();
                                    let is_dead = stored_dead || (now - last_response) > (48 * 3600);

                                    // If dead and haven't resurrected, alive_days should be 0
                                    let alive_days = if is_dead {
                                        0.0
                                    } else {
                                        let alive_seconds = last_response - last_alive_start;
                                        alive_seconds as f64 / 86400.0
                                    };

                                    user_obj.insert("alive_days".to_string(), serde_json::json!(alive_days));
                                    user_obj.insert("is_dead".to_string(), serde_json::json!(is_dead));
                                }
                                user_obj.insert("last_alive_start".to_string(), serde_json::json!(last_alive_start));
                            }
                        }
                    }
                }

                Some(serde_json::Value::Object(state_obj))
            })
    }) {
        Ok(aydy_states) => HttpResponse::Ok().json(serde_json::json!({
            "aydy": aydy_states,
            "count": aydy_states.len()
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

#[get("/api/aydy/leaderboard")]
pub async fn get_aydy_leaderboard() -> impl Responder {
    match crate::db::read_table(crate::AYDY, |_key, value| {
        // Parse the JSON value to extract AYDY state
        serde_json::from_str::<serde_json::Value>(value)
            .ok()
            .and_then(|state| {
                // Extract enrolled_users from the state
                state.get("enrolled_users").and_then(|users| {
                    users.as_object().map(|user_map| {
                        user_map
                            .iter()
                            .filter_map(|(_user_id, user_data)| {
                                let user_obj = user_data.as_object()?;
                                let user_name = user_obj.get("user_name")?.as_str()?.to_string();
                                let last_response = user_obj.get("last_response")?.as_i64()?;
                                let enrolled_at = user_obj.get("enrolled_at")?.as_i64()?;
                                let death_count = user_obj.get("death_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let last_alive_start = match user_obj.get("last_alive_start") {
                                    Some(v) => {
                                        let timestamp = v.as_i64().unwrap_or(0);
                                        if timestamp == 0 { 1769904000 } else { timestamp }
                                    },
                                    None => 1769904000 // Feb 1, 2026 00:00:00 UTC for legacy users
                                };

                                // A user is dead if the stored flag says so OR they haven't responded in 48h.
                                // We must OR both conditions because is_currently_dead may be stale/false
                                // for legacy users whose state was never updated by the bot timer.
                                let stored_dead = user_obj.get("is_currently_dead")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                let now = chrono::Utc::now().timestamp();
                                let is_dead = stored_dead || (now - last_response) > (48 * 3600);

                                // If dead and haven't resurrected, alive_days should be 0
                                let alive_days = if is_dead {
                                    0.0
                                } else {
                                    let alive_seconds = last_response - last_alive_start;
                                    alive_seconds as f64 / 86400.0
                                };

                                Some(LeaderboardEntry {
                                    user_name,
                                    alive_days,
                                    death_count,
                                    last_response,
                                    enrolled_at,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                })
            })
    }) {
        Ok(all_users) => {
            // Flatten all users from all channels
            let mut leaderboard: Vec<LeaderboardEntry> = all_users
                .into_iter()
                .flat_map(|users| users)
                .collect();

            // Sort by alive_days descending
            leaderboard.sort_by(|a, b| {
                b.alive_days.partial_cmp(&a.alive_days).unwrap_or(std::cmp::Ordering::Equal)
            });

            HttpResponse::Ok().json(serde_json::json!({
                "leaderboard": leaderboard,
                "count": leaderboard.len()
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}
