use std::sync::Arc;

use dashmap::Entry;
use nanoid::nanoid;
use rocket::{State, futures::StreamExt, get, http::Status, post, serde::json::Json};
use rocket_ws::{Channel, Message, WebSocket};
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, warn};

use minesweeper_common::{
    models::{CreateResponse, GameParams},
    protocol::ClientMessage,
};

use crate::{
    logic::{Game, Games},
    rate_limit::{ClientIp, RateLimiter, check_rate_limit},
};

#[instrument(level = "trace", skip(games, game))]
fn add_game(games: &State<Games>, game: Game) -> String {
    let mut id_length = 5;
    let max_attempts_per_length = 10;

    loop {
        for _ in 0..max_attempts_per_length {
            let id = nanoid!(id_length);
            match games.entry(id.clone()) {
                Entry::Occupied(_) => {
                    debug!("Game ID collision, trying another: {}", id);
                    continue;
                }
                Entry::Vacant(entry) => {
                    entry.insert(Arc::new(Mutex::new(game)));
                    info!("Created new game with ID: {}", id);
                    return id;
                }
            }
        }

        warn!(
            "Exhausted ID attempts at length {}, increasing to {}",
            id_length,
            id_length + 1
        );
        id_length += 1;
    }
}

#[post("/create", data = "<params>")]
#[instrument(level = "trace", skip(games, rate_limiter), fields(client_ip = %client_ip.0, width = params.width, height = params.height, bombs = params.bombs))]
pub fn create_game(
    params: Json<GameParams>,
    games: &State<Games>,
    rate_limiter: &State<RateLimiter>,
    client_ip: ClientIp,
) -> Result<Json<CreateResponse>, Status> {
    info!(
        "Game creation request from {}: {}x{} with {} bombs",
        client_ip.0, params.width, params.height, params.bombs
    );

    if let Err(status) = check_rate_limit(rate_limiter, &client_ip) {
        warn!("Rate limit exceeded for client {}", client_ip.0);
        return Err(status);
    }

    let game = Game::new(params.0);
    let id = add_game(games, game);

    info!(
        "Successfully created game {} for client {}",
        id, client_ip.0
    );
    Ok(Json(CreateResponse { id }))
}

#[get("/ws?<id>")]
#[instrument(level = "trace", skip(ws, games), fields(game_id = %id))]
pub fn websocket_handler(
    ws: WebSocket,
    games: &State<Games>,
    id: String,
) -> Result<Channel<'static>, Status> {
    let game = match games.get(&id) {
        None => {
            warn!("WebSocket connection attempt for non-existent game: {}", id);
            return Err(Status::NotFound);
        }
        Some(value) => {
            info!("WebSocket connection established for game: {}", id);
            value.value().clone()
        }
    };

    Ok(ws.channel(move |stream| {
        let game_id = id.clone();
        Box::pin(async move {
            let (write, mut read) = stream.split();

            let stream_id = {
                let mut game = game.lock().await;
                game.add_stream(write).await
            };

            info!(
                "Client connected to game {} (stream: {})",
                game_id, stream_id
            );

            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(message) => {
                            debug!("Received message from game {}: {:?}", game_id, message);
                            match message {
                                ClientMessage::Reveal { pos } => {
                                    debug!(
                                        "Player revealing cell at ({}, {}) in game {}",
                                        pos.x, pos.y, game_id
                                    );
                                    let mut game = game.lock().await;
                                    game.reveal(pos).await;
                                }
                                ClientMessage::Flag { pos } => {
                                    debug!(
                                        "Player flagging cell at ({}, {}) in game {}",
                                        pos.x, pos.y, game_id
                                    );
                                    let mut game = game.lock().await;
                                    game.flag(pos).await;
                                }
                                ClientMessage::Restart { params } => {
                                    info!(
                                        "Player restarting game {}: {}x{} with {} bombs",
                                        game_id, params.width, params.height, params.bombs
                                    );
                                    let mut game = game.lock().await;
                                    game.restart(params).await;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Invalid message format in game {}: {} - Error: {}",
                                game_id, text, e
                            );
                        }
                    },
                    Ok(Message::Close(_)) => {
                        info!(
                            "WebSocket connection closed for game {} (stream: {})",
                            game_id, stream_id
                        );
                        break;
                    }
                    Err(e) => {
                        error!(
                            "WebSocket error in game {} (stream: {}): {}",
                            game_id, stream_id, e
                        );
                        break;
                    }
                    _ => {
                        debug!("Received non-text message in game {}, ignoring", game_id);
                        break;
                    }
                }
            }

            {
                let mut game = game.lock().await;
                game.remove_stream(&stream_id).await;
            }

            info!(
                "Client disconnected from game {} (stream: {})",
                game_id, stream_id
            );
            Ok(())
        })
    }))
}
