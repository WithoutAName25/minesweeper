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

/// Connection state - all fields are required when connected
struct ConnectionState {
    websocket_sender: mpsc::UnboundedSender<ClientMessage>,
    game_id: String,
    background_task: JoinHandle<()>,
}

impl ConnectionState {
    /// Send a message through the WebSocket connection
    fn send_message(&self, message: ClientMessage) -> Result<()> {
        self.websocket_sender
            .send(message)
            .map_err(|_| "WebSocket sender closed")?;
        Ok(())
    }

    /// Get the game ID
    fn get_game_id(&self) -> &String {
        &self.game_id
    }

    /// Abort the background task and wait for it to finish
    async fn abort_and_wait_background_task(self) {
        self.background_task.abort();
        let _ = self.background_task.await;
    }
}

/// High-level minesweeper game client that manages game state locally
pub struct MinesweeperGame {
    client: MinesweeperClient,
    connection_state: Arc<RwLock<Option<ConnectionState>>>,
    event_sender: Arc<RwLock<Option<mpsc::UnboundedSender<GameEvent>>>>,
    state: Arc<RwLock<Option<GameState>>>,
}

impl MinesweeperGame {
    /// Create a new game instance
    pub fn new(server_url: &str) -> Result<Self> {
        let client = MinesweeperClient::new(server_url)?;
        Ok(Self {
            client,
            connection_state: Arc::new(RwLock::new(None)),
            event_sender: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(None)),
        })
    }

    /// Subscribe to game events. Returns a receiver for game events.
    pub async fn subscribe_to_events(&self) -> mpsc::UnboundedReceiver<GameEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut event_sender = self.event_sender.write().await;
        *event_sender = Some(sender);
        receiver
    }

    /// Start a new game with the specified parameters
    pub async fn start_game(&self, params: GameParams) -> Result<()> {
        info!(
            "Starting new game: {}x{} with {} bombs",
            params.width, params.height, params.bombs
        );

        // Create the game via HTTP API
        let game_id = self.client.create_game(params).await?;
        info!("Created game with ID: {}", game_id);

        self.join_game(game_id).await
    }

    pub async fn join_game(&self, game_id: String) -> Result<()> {
        info!("Joining game with ID: {}", game_id);

        let mut conn_state = self.connection_state.write().await;

        // Stop any existing background task
        if let Some(existing_conn) = conn_state.take() {
            existing_conn.abort_and_wait_background_task().await;
        }
        self.state.write().await.take();

        // Connect to the game via WebSocket
        let ws_url = self.client.websocket_url(&game_id)?;
        let websocket = MinesweeperWebSocket::connect(&ws_url).await?;
        let websocket_sender = websocket.get_sender();

        info!("Connected to game with ID: {}", game_id);

        // Start background message listener
        let background_task = self.start_background_listener(websocket);

        // Create new connection state
        *conn_state = Some(ConnectionState {
            websocket_sender,
            game_id,
            background_task,
        });

        Ok(())
    }

    /// Send a message to the connected game
    async fn send_client_message(&self, message: ClientMessage) -> Result<()> {
        let conn_state = self.connection_state.read().await;

        if let Some(ref conn) = *conn_state {
            conn.send_message(message)?;
        } else {
            return Err("Not connected to a game. Call start_game() first.".into());
        }

        Ok(())
    }

    /// Reveal a cell at the specified position
    pub async fn reveal(&self, pos: Pos) -> Result<()> {
        debug!("Revealing cell at ({}, {})", pos.x, pos.y);

        let message = ClientMessage::Reveal { pos };
        self.send_client_message(message).await
    }

    /// Flag/unflag a cell at the specified position
    pub async fn flag(&self, pos: Pos) -> Result<()> {
        debug!("Flagging cell at ({}, {})", pos.x, pos.y);

        let message = ClientMessage::Flag { pos };
        self.send_client_message(message).await
    }

    /// Restart the game with new parameters
    pub async fn restart(&self, params: GameParams) -> Result<()> {
        info!(
            "Restarting game with new parameters: {}x{} with {} bombs",
            params.width, params.height, params.bombs
        );

        let message = ClientMessage::Restart { params };
        self.send_client_message(message).await
    }

    /// Get the current game state
    pub async fn get_state(&self) -> Option<GameState> {
        self.state.read().await.clone()
    }

    /// Get the game ID
    pub async fn get_game_id(&self) -> Option<String> {
        let conn_state = self.connection_state.read().await;
        conn_state.as_ref().map(|conn| conn.get_game_id().clone())
    }

    /// Check if we're connected to a game
    pub async fn is_connected(&self) -> bool {
        let conn_state = self.connection_state.read().await;
        conn_state.is_some()
    }

    /// Close the connection and clean up
    pub async fn disconnect(&self) -> Result<()> {
        let mut conn_state = self.connection_state.write().await;

        if let Some(conn) = conn_state.take() {
            conn.abort_and_wait_background_task().await;
        }

        // Clear event sender
        *self.event_sender.write().await = None;

        // Clear game state
        *self.state.write().await = None;

        info!("Disconnected from game");
        Ok(())
    }

    /// Start background WebSocket message listener
    fn start_background_listener(&self, mut websocket: MinesweeperWebSocket) -> JoinHandle<()> {
        let state = self.state.clone();
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            Self::background_message_handler(&mut websocket, state, event_sender).await;
        })
    }

    /// Background task that handles incoming WebSocket messages
    async fn background_message_handler(
        websocket: &mut MinesweeperWebSocket,
        state: Arc<RwLock<Option<GameState>>>,
        event_sender: Arc<RwLock<Option<mpsc::UnboundedSender<GameEvent>>>>,
    ) {
        loop {
            let message = match websocket.receive_message().await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    // Connection closed
                    if let Some(ref sender) = *event_sender.read().await {
                        let _ = sender.send(GameEvent::ConnectionLost);
                    }
                    break;
                }
                Err(e) => {
                    warn!("Error receiving WebSocket message: {}", e);
                    if let Some(ref sender) = *event_sender.read().await {
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

                    if let Some(ref sender) = *event_sender.read().await {
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

                    if let Some(ref sender) = *event_sender.read().await {
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
}
