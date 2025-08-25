use dashmap::DashMap;
use minesweeper_server::{
    cors::create_cors,
    logic::Games,
    routes::{create_game, websocket_handler},
};
use rocket::{Build, Rocket, routes};

#[rocket::launch]
fn rocket() -> Rocket<Build> {
    tracing_subscriber::fmt::init();

    let games: Games = DashMap::new();

    rocket::build()
        .attach(create_cors())
        .manage(games)
        .mount("/", routes![create_game, websocket_handler])
}
