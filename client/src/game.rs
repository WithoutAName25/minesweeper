use std::collections::HashMap;
use std::sync::Arc;

use minesweeper_common::{
    models::{Cell, GameParams, Pos},
    protocol::{ClientMessage, ServerMessage},
};
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::{MinesweeperClient, MinesweeperWebSocket, Result};

/// Events emitted by the minesweeper game
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// The game board was updated with new cell states
    BoardUpdated {
        /// List of cell positions that changed
        changed_positions: Vec<Pos>,
    },
    /// Game status changed (won/lost)
    GameStatusChanged { won: bool, lost: bool },
    /// Game was initialized or restarted
    GameInitialized {
        width: usize,
        height: usize,
        bombs: usize,
    },
    /// Connection was lost
    ConnectionLost,
}

/// Represents the current state of a minesweeper game
#[derive(Debug, Clone)]
pub struct GameState {
    pub width: usize,
    pub height: usize,
    pub bombs: usize,
    pub board: Vec<Vec<Cell>>,
    pub game_over: bool,
    pub won: bool,
}

impl GameState {
    /// Create a new game state
    pub fn new(width: usize, height: usize, bombs: usize, board: Vec<Vec<Cell>>) -> Self {
        Self {
            width,
            height,
            bombs,
            board,
            game_over: false,
            won: false,
        }
    }

    /// Get the cell at the specified position
    pub fn get_cell(&self, pos: Pos) -> Option<&Cell> {
        if pos.x < self.width && pos.y < self.height {
            self.board.get(pos.y)?.get(pos.x)
        } else {
            None
        }
    }

    /// Update a cell at the specified position
    pub fn set_cell(&mut self, pos: Pos, cell: Cell) {
        if pos.x < self.width
            && pos.y < self.height
            && let Some(row) = self.board.get_mut(pos.y)
            && let Some(cell_ref) = row.get_mut(pos.x)
        {
            *cell_ref = cell;
        }
    }

    /// Count the number of cells in each state
    pub fn count_cells(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for row in &self.board {
            for cell in row {
                let state = match cell {
                    Cell::Hidden => "hidden",
                    Cell::Marked => "marked",
                    Cell::Flagged => "flagged",
                    Cell::Revealed { .. } => "revealed",
                    Cell::Bomb => "bomb",
                };
                *counts.entry(state.to_string()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Check if the game is in a completed state (won or lost)
    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    /// Check if the player has won
    pub fn is_won(&self) -> bool {
        self.won
    }
}

/// High-level minesweeper game client that manages game state locally
pub struct MinesweeperGame {
    client: MinesweeperClient,
    websocket_sender: Option<mpsc::UnboundedSender<ClientMessage>>,
    game_id: Option<String>,
    state: Arc<RwLock<Option<GameState>>>,
    event_sender: Option<mpsc::UnboundedSender<GameEvent>>,
    background_task: Option<JoinHandle<()>>,
}

impl MinesweeperGame {
    /// Create a new game instance
    pub fn new(server_url: &str) -> Result<Self> {
        let client = MinesweeperClient::new(server_url)?;
        Ok(Self {
            client,
            websocket_sender: None,
            game_id: None,
            state: Arc::new(RwLock::new(None)),
            event_sender: None,
            background_task: None,
        })
    }

    /// Subscribe to game events. Returns a receiver for game events.
    pub fn subscribe_to_events(&mut self) -> mpsc::UnboundedReceiver<GameEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.event_sender = Some(sender);
        receiver
    }

    /// Start a new game with the specified parameters
    pub async fn start_game(&mut self, params: GameParams) -> Result<()> {
        info!(
            "Starting new game: {}x{} with {} bombs",
            params.width, params.height, params.bombs
        );

        // Stop any existing background task
        self.stop_background_listener().await;

        // Create the game via HTTP API
        let game_id = self.client.create_game(params).await?;
        info!("Created game with ID: {}", game_id);

        // Connect to the game via WebSocket
        let ws_url = self.client.websocket_url(&game_id)?;
        let websocket = MinesweeperWebSocket::connect(&ws_url).await?;

        self.game_id = Some(game_id);
        self.websocket_sender = Some(websocket.get_sender());

        // Start background message listener
        self.start_background_listener(websocket);

        Ok(())
    }

    /// Reveal a cell at the specified position
    pub async fn reveal(&mut self, x: usize, y: usize) -> Result<()> {
        self.ensure_connected()?;

        let pos = Pos { x, y };
        debug!("Revealing cell at ({}, {})", x, y);

        let message = ClientMessage::Reveal { pos };
        if let Some(ref sender) = self.websocket_sender {
            sender
                .send(message)
                .map_err(|_| "WebSocket sender closed")?;
        }

        Ok(())
    }

    /// Flag/unflag a cell at the specified position
    pub async fn flag(&mut self, x: usize, y: usize) -> Result<()> {
        self.ensure_connected()?;

        let pos = Pos { x, y };
        debug!("Flagging cell at ({}, {})", x, y);

        let message = ClientMessage::Flag { pos };
        if let Some(ref sender) = self.websocket_sender {
            sender
                .send(message)
                .map_err(|_| "WebSocket sender closed")?;
        }

        Ok(())
    }

    /// Restart the game with new parameters
    pub async fn restart(&mut self, params: GameParams) -> Result<()> {
        self.ensure_connected()?;

        info!(
            "Restarting game with new parameters: {}x{} with {} bombs",
            params.width, params.height, params.bombs
        );

        let message = ClientMessage::Restart { params };
        if let Some(ref sender) = self.websocket_sender {
            sender
                .send(message)
                .map_err(|_| "WebSocket sender closed")?;
        }

        Ok(())
    }

    /// Get the current game state
    pub async fn get_state(&self) -> Option<GameState> {
        self.state.read().await.clone()
    }

    /// Get the game ID
    pub fn get_game_id(&self) -> Option<&String> {
        self.game_id.as_ref()
    }

    /// Check if we're connected to a game
    pub fn is_connected(&self) -> bool {
        self.websocket_sender.is_some() && self.game_id.is_some()
    }

    /// Close the connection and clean up
    pub async fn disconnect(&mut self) -> Result<()> {
        self.stop_background_listener().await;

        // Drop the sender to close the WebSocket
        self.websocket_sender = None;

        self.game_id = None;
        *self.state.write().await = None;
        self.event_sender = None;

        info!("Disconnected from game");
        Ok(())
    }

    /// Start background WebSocket message listener
    fn start_background_listener(&mut self, mut websocket: MinesweeperWebSocket) {
        let state = self.state.clone();
        let event_sender = self.event_sender.clone();

        let handle = tokio::spawn(async move {
            Self::background_message_handler(&mut websocket, state, event_sender).await;
        });
        self.background_task = Some(handle);
    }

    /// Stop background WebSocket message listener
    async fn stop_background_listener(&mut self) {
        if let Some(handle) = self.background_task.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Background task that handles incoming WebSocket messages
    async fn background_message_handler(
        websocket: &mut MinesweeperWebSocket,
        state: Arc<RwLock<Option<GameState>>>,
        event_sender: Option<mpsc::UnboundedSender<GameEvent>>,
    ) {
        loop {
            let message = match websocket.receive_message().await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    // Connection closed
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(GameEvent::ConnectionLost);
                    }
                    break;
                }
                Err(e) => {
                    warn!("Error receiving WebSocket message: {}", e);
                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(GameEvent::ConnectionLost);
                    }
                    break;
                }
            };

            match message {
                ServerMessage::Init {
                    width,
                    height,
                    bombs,
                    field,
                } => {
                    info!(
                        "Received game initialization: {}x{} with {} bombs",
                        width, height, bombs
                    );

                    let new_state = GameState::new(width, height, bombs, field);
                    *state.write().await = Some(new_state);

                    if let Some(ref sender) = event_sender {
                        let _ = sender.send(GameEvent::GameInitialized {
                            width,
                            height,
                            bombs,
                        });
                    }
                }
                ServerMessage::Update { updates, won, lost } => {
                    debug!(
                        "Received update: {} cells updated, won: {}, lost: {}",
                        updates.len(),
                        won,
                        lost
                    );

                    let changed_positions: Vec<Pos> = updates.iter().map(|u| u.pos).collect();
                    let status_changed;

                    {
                        let mut state_guard = state.write().await;
                        if let Some(ref mut game_state) = *state_guard {
                            let old_won = game_state.won;
                            let old_game_over = game_state.game_over;

                            // Apply updates to local board
                            for update in updates {
                                game_state.set_cell(update.pos, update.value);
                            }

                            // Update game status
                            game_state.won = won;
                            game_state.game_over = won || lost;

                            status_changed =
                                game_state.won != old_won || game_state.game_over != old_game_over;
                        } else {
                            status_changed = false;
                        }
                    }

                    if let Some(ref sender) = event_sender {
                        if !changed_positions.is_empty() {
                            let _ = sender.send(GameEvent::BoardUpdated { changed_positions });
                        }

                        if status_changed {
                            let _ = sender.send(GameEvent::GameStatusChanged { won, lost });
                        }
                    }
                }
            }
        }
    }

    /// Ensure we're connected to a game
    fn ensure_connected(&self) -> Result<()> {
        if !self.is_connected() {
            return Err("Not connected to a game. Call start_game() first.".into());
        }
        Ok(())
    }
}
