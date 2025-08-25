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
            let games_for_cleanup = games.clone();
            tokio::spawn(async move {
                start_cleanup_task(games_for_cleanup).await;
            });
        }
        Ok(rocket)
    }
}

#[rocket::launch]
fn rocket() -> Rocket<Build> {
    tracing_subscriber::fmt::init();

    let games: Games = Arc::new(DashMap::new());
    let rate_limiter = create_rate_limiter();

    rocket::build()
        .attach(create_cors())
        .attach(CleanupFairing)
        .manage(games)
        .manage(rate_limiter)
        .mount("/", routes![create_game, websocket_handler])
}
