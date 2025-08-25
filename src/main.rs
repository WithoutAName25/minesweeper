use dashmap::DashMap;
use minesweeper_server::{
    cleanup::start_cleanup_task,
    cors::create_cors,
    logic::Games,
    rate_limit::create_rate_limiter,
    routes::{create_game, websocket_handler},
};
use rocket::{Build, Rocket, routes};

#[rocket::launch]
fn rocket() -> Rocket<Build> {
    tracing_subscriber::fmt::init();

    let games: Games = DashMap::new();
    let rate_limiter = create_rate_limiter();

    // Start cleanup task in background
    let games_for_cleanup = games.clone();
    tokio::spawn(async move {
        start_cleanup_task(games_for_cleanup).await;
    });

    rocket::build()
        .attach(create_cors())
        .manage(games)
        .manage(rate_limiter)
        .mount("/", routes![create_game, websocket_handler])
}
