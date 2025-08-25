use std::sync::Arc;

use dashmap::Entry;
use nanoid::nanoid;
use rocket::{State, futures::StreamExt, get, http::Status, post, serde::json::Json};
use rocket_ws::{Channel, Message, WebSocket};
use tokio::sync::Mutex;

use crate::{
    logic::{Game, Games},
    model::{GameParams, api::CreateResponse, client::ClientMessage},
};

fn add_game(games: &State<Games>, game: Game) -> String {
    let mut id_length = 5;
    let max_attempts_per_length = 10;

    loop {
        for _ in 0..max_attempts_per_length {
            let id = nanoid!(id_length);
            match games.entry(id.clone()) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    entry.insert(Arc::new(Mutex::new(game)));
                    return id;
                }
            }
        }

        id_length += 1;
    }
}

#[post("/create", data = "<params>")]
pub fn create_game(params: Json<GameParams>, games: &State<Games>) -> Json<CreateResponse> {
    let game = Game::new(params.0);
    let id = add_game(games, game);
    Json(CreateResponse { id })
}

#[get("/ws?<id>")]
pub fn websocket_handler(
    ws: WebSocket,
    games: &State<Games>,
    id: String,
) -> Result<Channel<'static>, Status> {
    let game = match games.get(&id) {
        None => return Err(Status::NotFound),
        Some(value) => value.value().clone(),
    };

    Ok(ws.channel(move |stream| {
        Box::pin(async move {
            let (write, mut read) = stream.split();

            let id = {
                let mut game = game.lock().await;
                game.add_stream(write).await
            };

            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        if let Ok(message) = serde_json::from_str::<ClientMessage>(&text) {
                            match message {
                                ClientMessage::Reveal { pos } => {
                                    let mut game = game.lock().await;
                                    game.reveal(pos).await;
                                }
                                ClientMessage::Flag { pos } => {
                                    let mut game = game.lock().await;
                                    game.flag(pos).await;
                                }
                                ClientMessage::Restart { params } => {
                                    let mut game = game.lock().await;
                                    game.restart(params).await;
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }

            {
                let mut game = game.lock().await;
                game.remove_stream(&id).await;
            }
            Ok(())
        })
    }))
}
