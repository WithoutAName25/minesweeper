use dashmap::DashMap;
use minesweeper_server::{
    cleanup::start_cleanup_task,
    cors::create_cors,
    logic::Games,
    rate_limit::create_rate_limiter,
    routes::{create_game, websocket_handler},
};
use rocket::{
    Build, Rocket,
    fairing::{Fairing, Info, Kind},
    routes,
};
use std::sync::Arc;
use tracing::{info, warn};

struct CleanupFairing;

#[rocket::async_trait]
impl Fairing for CleanupFairing {
    fn info(&self) -> Info {
        Info {
            name: "Cleanup Task",
            kind: Kind::Ignite,
        }
    }

    async fn on_ignite(&self, rocket: Rocket<Build>) -> rocket::fairing::Result {
        if let Some(games) = rocket.state::<Games>() {
            info!("Starting cleanup task for game management");
            let games_for_cleanup = games.clone();
            tokio::spawn(async move {
                start_cleanup_task(games_for_cleanup).await;
            });
        } else {
            warn!("Failed to get games state for cleanup task");
        }
        Ok(rocket)
    }
}

#[rocket::launch]
fn rocket() -> Rocket<Build> {
    tracing_subscriber::fmt::init();
    info!("🚀 Starting Minesweeper multiplayer server");

    let games: Games = Arc::new(DashMap::new());
    let rate_limiter = create_rate_limiter();

    info!("📊 Initialized game storage and rate limiter");

    let rocket = rocket::build()
        .attach(create_cors())
        .attach(CleanupFairing)
        .manage(games)
        .manage(rate_limiter)
        .mount("/", routes![create_game, websocket_handler]);

    info!("🌐 Server configured with CORS, cleanup task, and routes");
    info!("📡 Endpoints: POST /create, GET /ws");

    rocket
}
