mod services;

use actix_files::{Files, NamedFile};
use actix_web::{App, HttpServer, Result as ActixResult, web};
use miette::Result;
use std::path::PathBuf;

async fn spa_fallback() -> ActixResult<NamedFile> {
    Ok(NamedFile::open(PathBuf::from("./static/index.html"))?)
}

pub async fn start_web_server(subsys: &mut tokio_graceful_shutdown::SubsystemHandle) -> Result<()> {
    log::info!("Starting web server...");

    let server = HttpServer::new(|| {
        App::new()
            // General endpoints
            .service(services::general::stats)
            .service(services::general::health)
            .service(services::general::logs)
            .service(services::general::commands)
            .service(services::general::emojis)
            .service(services::general::history)
            // Tags endpoints
            .service(services::tags::get_tags)
            .service(services::tags::create_tag)
            .service(services::tags::update_tag)
            .service(services::tags::delete_tag)
            // AYDY endpoints
            .service(services::aydy::get_aydy)
            .service(services::aydy::get_aydy_leaderboard)
            // Ticket endpoints
            .service(services::tickets::get_tickets)
            .service(services::tickets::get_ticket)
            .service(services::tickets::create_ticket)
            .service(services::tickets::update_ticket)
            .service(services::tickets::delete_ticket)
            // Channel endpoints
            .service(services::channels::get_channels)
            // Static files
            .service(
                Files::new("/", "./static")
                    .index_file("index.html")
                    .use_last_modified(true)
                    .default_handler(web::to(spa_fallback)),
            )
    })
    .bind(("0.0.0.0", 8080))
    .map_err(|e| miette::miette!("Failed to bind web server: {}", e))?;

    log::info!("Web server listening on http://0.0.0.0:8080");

    let server_handle = server.run();
    let server_task = tokio::spawn(server_handle);

    subsys.on_shutdown_requested().await;
    log::info!("Web server shutting down...");

    server_task.abort();

    Ok(())
}
