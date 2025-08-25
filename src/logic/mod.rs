use std::{collections::HashMap, sync::Arc, time::Instant};

use dashmap::DashMap;
use rand::Rng;
use rocket::futures::{SinkExt, future::join_all, stream::SplitSink};
use rocket_ws::{Message, stream::DuplexStream};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    data::{Cell, Field, RevealedState},
    model::{
        self, GameParams, Pos,
        server::{CellUpdate, ServerMessage},
    },
};

pub type Games = Arc<DashMap<String, Arc<Mutex<Game>>>>;

pub struct Game {
    field: Field,
    streams: HashMap<Uuid, SplitSink<DuplexStream, Message>>,
    last_activity: Instant,
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

impl From<&Cell> for model::Cell {
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
    fn new(params: GameParams) -> Self {
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
                .collect::<Vec<model::Cell>>()
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
    pub fn new(params: GameParams) -> Self {
        Self {
            field: Field::new(params),
            streams: HashMap::new(),
            last_activity: Instant::now(),
        }
    }

    pub async fn restart(&mut self, params: GameParams) {
        self.field = Field::new(params);
        self.last_activity = Instant::now();
        broadcast(&mut self.streams, &self.field.init_message()).await;
    }

    pub async fn add_stream(&mut self, mut stream: SplitSink<DuplexStream, Message>) -> Uuid {
        let id = Uuid::new_v4();
        send(&mut stream, &self.field.init_message()).await;
        self.streams.insert(id, stream);
        self.last_activity = Instant::now();
        id
    }

    pub async fn remove_stream(&mut self, id: &Uuid) {
        self.streams.remove(id);
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

    pub async fn flag(&mut self, pos: Pos) {
        if !self.field.validate_pos(&pos) || self.field.finished {
            return;
        }

        self.last_activity = Instant::now();

        if let Some(cell) = self.field.cells.get_mut(pos.x + pos.y * self.field.width) {
            match cell.revealed {
                RevealedState::Hidden => cell.revealed = RevealedState::Flagged,
                RevealedState::Marked => cell.revealed = RevealedState::Hidden,
                RevealedState::Flagged => cell.revealed = RevealedState::Marked,
                RevealedState::Revealed => return,
            };
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
        };
    }

    pub async fn reveal(&mut self, pos: Pos) {
        if !self.field.validate_pos(&pos) || self.field.finished {
            return;
        }

        self.last_activity = Instant::now();

        if let Some(cell) = self.field.cells.get_mut(pos.x + pos.y * self.field.width) {
            if cell.revealed == RevealedState::Flagged {
                return;
            }

            if cell.bomb {
                let mut updates = Vec::new();
                self.field.reveal_bombs(&mut updates);
                self.field.finished = true;
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

            let mut updates = Vec::new();
            self.field.reveal_recursive(pos, &mut updates);
            let won = self.field.has_won();
            self.field.finished = won;
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
