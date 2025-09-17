use std::{cmp::min, collections::HashMap, sync::Arc, time::Instant};

use dashmap::DashMap;
use rand::Rng;
use rocket::futures::{SinkExt, future::join_all, stream::SplitSink};
use rocket_ws::{Message, stream::DuplexStream};
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use minesweeper_common::{
    models::{GameParams, Pos},
    protocol::{CellUpdate, ServerMessage},
};

use crate::data::{Cell, Field, RevealedState};

pub type Games = Arc<DashMap<String, Arc<Mutex<Game>>>>;

pub struct Game {
    field: Field,
    streams: HashMap<Uuid, SplitSink<DuplexStream, Message>>,
    last_activity: Instant,
}

fn validate_params(params: &mut GameParams) {
    params.bombs = min(params.bombs, params.width * params.height)
}

fn generate_bombs(params: &GameParams) -> Vec<bool> {
    let mut bombs = Vec::new();
    let mut rng = rand::rng();

    let mut bombs_left = params.bombs;
    let length = params.width * params.height;
    for cells_left in (1..=length).rev() {
        let value = rng.random_ratio(bombs_left as u32, cells_left as u32);
        bombs.push(value);
        if value {
            bombs_left -= 1;
        }
    }

    bombs
}

fn count_adjacent_bombs(bombs: &[bool], index: usize, params: &GameParams) -> u8 {
    let x = index % params.width;
    let y = index / params.width;
    let mut count = 0;

    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }

            let new_x = x as i32 + dx;
            let new_y = y as i32 + dy;

            if new_x >= 0
                && new_x < params.width as i32
                && new_y >= 0
                && new_y < params.height as i32
            {
                let adj_index = (new_x as usize) + (new_y as usize) * params.width;
                if adj_index < bombs.len() && bombs[adj_index] {
                    count += 1;
                }
            }
        }
    }

    count
}

fn generate_cells(params: &GameParams) -> Vec<Cell> {
    let bombs = generate_bombs(params);
    let cells = bombs.iter().enumerate().map(|(i, bomb)| Cell {
        bomb: *bomb,
        adjacent: count_adjacent_bombs(&bombs, i, params),
        revealed: RevealedState::Hidden,
    });

    cells.collect()
}

impl From<&Cell> for minesweeper_common::models::Cell {
    fn from(value: &Cell) -> Self {
        match value.revealed {
            RevealedState::Hidden => Self::Hidden,
            RevealedState::Marked => Self::Marked,
            RevealedState::Flagged => Self::Flagged,
            RevealedState::Revealed if value.bomb => Self::Bomb,
            RevealedState::Revealed => Self::Revealed {
                adjacent: value.adjacent,
            },
        }
    }
}

async fn send(stream: &mut SplitSink<DuplexStream, Message>, message: &ServerMessage) {
    if let Ok(text) = serde_json::to_string(message) {
        let _ = stream.send(Message::Text(text)).await;
    }
}
async fn broadcast(
    streams: &mut HashMap<Uuid, SplitSink<DuplexStream, Message>>,
    message: &ServerMessage,
) {
    let futures: Vec<_> = streams
        .iter_mut()
        .map(|(_, stream)| send(stream, message))
        .collect();

    join_all(futures).await;
}

impl Field {
    fn new(mut params: GameParams) -> Self {
        validate_params(&mut params);
        Self {
            width: params.width,
            height: params.height,
            bombs: params.bombs,
            revealed: 0,
            finished: false,
            cells: generate_cells(&params),
        }
    }

    fn init_message(&self) -> ServerMessage {
        ServerMessage::Init {
            width: self.width,
            height: self.height,
            bombs: self.bombs,
            field: self
                .cells
                .iter()
                .map(|cell| cell.into())
                .collect::<Vec<minesweeper_common::models::Cell>>()
                .chunks(self.width)
                .map(|chunk| chunk.to_vec())
                .collect(),
        }
    }

    fn has_won(&self) -> bool {
        self.width * self.height == self.bombs + self.revealed
    }

    fn reveal_bombs(&mut self, updates: &mut Vec<CellUpdate>) {
        for y in 0..self.height {
            for x in 0..self.width {
                let pos = Pos { x, y };

                if let Some(cell) = self.cells.get_mut(pos.x + pos.y * self.width)
                    && cell.bomb
                {
                    cell.revealed = RevealedState::Revealed;
                    updates.push(CellUpdate {
                        pos,
                        value: (&*cell).into(),
                    });
                }
            }
        }
    }

    fn reveal_recursive(&mut self, pos: Pos, updates: &mut Vec<CellUpdate>) {
        if !self.validate_pos(&pos) {
            return;
        }

        if let Some(cell) = self.cells.get_mut(pos.x + pos.y * self.width) {
            if cell.revealed == RevealedState::Revealed {
                return;
            }

            cell.revealed = RevealedState::Revealed;
            self.revealed += 1;
            updates.push(CellUpdate {
                pos,
                value: (&*cell).into(),
            });

            if cell.adjacent != 0 {
                return;
            }

            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    self.reveal_recursive(
                        Pos {
                            x: (pos.x as i32 + dx) as usize,
                            y: (pos.y as i32 + dy) as usize,
                        },
                        updates,
                    );
                }
            }
        }
    }

    fn validate_pos(&self, pos: &Pos) -> bool {
        pos.x < self.width && pos.y < self.height
    }
}

impl Game {
    #[instrument(level = "trace")]
    pub fn new(params: GameParams) -> Self {
        info!(
            "Creating new game: {}x{} with {} bombs",
            params.width, params.height, params.bombs
        );
        Self {
            field: Field::new(params),
            streams: HashMap::new(),
            last_activity: Instant::now(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn restart(&mut self, params: GameParams) {
        info!(
            "Restarting game with new parameters: {}x{} with {} bombs",
            params.width, params.height, params.bombs
        );
        self.field = Field::new(params);
        self.last_activity = Instant::now();
        broadcast(&mut self.streams, &self.field.init_message()).await;
        info!(
            "Game restarted and broadcasted to {} connections",
            self.streams.len()
        );
    }

    #[instrument(level = "trace", skip(self, stream))]
    pub async fn add_stream(&mut self, mut stream: SplitSink<DuplexStream, Message>) -> Uuid {
        let id = Uuid::new_v4();
        debug!("Adding stream {} to game", id);
        send(&mut stream, &self.field.init_message()).await;
        self.streams.insert(id, stream);
        self.last_activity = Instant::now();
        info!(
            "Stream {} added, total connections: {}",
            id,
            self.streams.len()
        );
        id
    }

    #[instrument(level = "trace", skip(self))]
    pub async fn remove_stream(&mut self, id: &Uuid) {
        if self.streams.remove(id).is_some() {
            info!(
                "Stream {} removed, remaining connections: {}",
                id,
                self.streams.len()
            );
        } else {
            warn!("Attempted to remove non-existent stream: {}", id);
        }
        self.last_activity = Instant::now()
    }

    pub fn has_active_connections(&self) -> bool {
        !self.streams.is_empty()
    }

    pub fn should_cleanup(&self, inactive_timeout_secs: u64) -> bool {
        if self.has_active_connections() {
            return false;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_activity).as_secs();

        elapsed > inactive_timeout_secs
    }

    #[instrument(level = "trace", skip(self), fields(x = pos.x, y = pos.y))]
    pub async fn flag(&mut self, pos: Pos) {
        if !self.field.validate_pos(&pos) {
            warn!("Invalid flag position: ({}, {})", pos.x, pos.y);
            return;
        }

        if self.field.finished {
            debug!(
                "Ignoring flag action on finished game at ({}, {})",
                pos.x, pos.y
            );
            return;
        }

        self.last_activity = Instant::now();

        if let Some(cell) = self.field.cells.get_mut(pos.x + pos.y * self.field.width) {
            let old_state = cell.revealed;
            match cell.revealed {
                RevealedState::Hidden => {
                    cell.revealed = RevealedState::Flagged;
                    debug!("Cell ({}, {}) flagged", pos.x, pos.y);
                }
                RevealedState::Marked => {
                    cell.revealed = RevealedState::Hidden;
                    debug!("Cell ({}, {}) unmarked", pos.x, pos.y);
                }
                RevealedState::Flagged => {
                    cell.revealed = RevealedState::Marked;
                    debug!("Cell ({}, {}) marked", pos.x, pos.y);
                }
                RevealedState::Revealed => {
                    debug!(
                        "Ignoring flag action on revealed cell ({}, {})",
                        pos.x, pos.y
                    );
                    return;
                }
            };

            if old_state != cell.revealed {
                broadcast(
                    &mut self.streams,
                    &ServerMessage::Update {
                        updates: vec![CellUpdate {
                            pos,
                            value: (&*cell).into(),
                        }],
                        won: false,
                        lost: false,
                    },
                )
                .await;
            }
        };
    }

    #[instrument(level = "trace", skip(self), fields(x = pos.x, y = pos.y))]
    pub async fn reveal(&mut self, pos: Pos) {
        if !self.field.validate_pos(&pos) {
            warn!("Invalid reveal position: ({}, {})", pos.x, pos.y);
            return;
        }

        if self.field.finished {
            debug!(
                "Ignoring reveal action on finished game at ({}, {})",
                pos.x, pos.y
            );
            return;
        }

        self.last_activity = Instant::now();

        if let Some(cell) = self.field.cells.get_mut(pos.x + pos.y * self.field.width) {
            if cell.revealed == RevealedState::Flagged {
                debug!("Ignoring reveal on flagged cell ({}, {})", pos.x, pos.y);
                return;
            }

            if cell.bomb {
                warn!("Player hit bomb at ({}, {}) - game over!", pos.x, pos.y);
                let mut updates = Vec::new();
                self.field.reveal_bombs(&mut updates);
                self.field.finished = true;
                info!("Game ended with loss, revealed {} bombs", updates.len());
                broadcast(
                    &mut self.streams,
                    &ServerMessage::Update {
                        updates,
                        won: false,
                        lost: true,
                    },
                )
                .await;
                return;
            }

            debug!(
                "Revealing cell ({}, {}) with {} adjacent bombs",
                pos.x, pos.y, cell.adjacent
            );
            let mut updates = Vec::new();
            self.field.reveal_recursive(pos, &mut updates);
            let won = self.field.has_won();
            self.field.finished = won;

            if won {
                info!("Game won! All safe cells revealed.");
            } else {
                debug!("Revealed {} cells, game continues", updates.len());
            }

            broadcast(
                &mut self.streams,
                &ServerMessage::Update {
                    updates,
                    won,
                    lost: false,
                },
            )
            .await;
        }
    }
}
