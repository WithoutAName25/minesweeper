//! Minesweeper Client Library
//!
//! This library provides a Rust client for the minesweeper multiplayer server,
//! supporting both HTTP API calls and WebSocket connections for real-time gameplay.
//!
//! ## Usage
//!
//! ### High-Level Interface (Recommended)
//!
//! The `MinesweeperGame` struct provides a high-level interface that manages game state
//! locally and provides convenient methods for game actions:
//!
//! ```rust,no_run
//! use minesweeper_client::{MinesweeperGame, GameParams, Pos};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let game = MinesweeperGame::new("http://localhost:8000")?;
//!
//!     // Start a new game
//!     let params = GameParams { width: 8, height: 8, bombs: 10 };
//!     game.start_game(params).await?;
//!
//!     // Make moves
//!     game.reveal(Pos { x: 0, y: 0 }).await?;
//!     game.flag(Pos { x: 1, y: 1 }).await?;
//!     
//!     // Check game state
//!     if let Some(state) = game.get_state().await {
//!         println!("Game over: {}, Won: {}", state.is_game_over(), state.is_won());
//!     }
//!     
//!     game.disconnect().await?;
//!     Ok(())
//! }
//! ```
//!
//! ### Low-Level Interface
//!
//! For more control, you can use the low-level `MinesweeperClient` and `MinesweeperWebSocket`
//! directly:
//!
//! ```rust,no_run
//! use minesweeper_client::{MinesweeperClient, MinesweeperWebSocket, GameParams, ClientMessage, Pos};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let client = MinesweeperClient::new("http://localhost:8000")?;
//!     let game_id = client.create_game(GameParams { width: 8, height: 8, bombs: 10 }).await?;
//!     
//!     let ws_url = client.websocket_url(&game_id)?;
//!     let mut ws = MinesweeperWebSocket::connect(&ws_url).await?;
//!     
//!     // Receive initial state
//!     if let Some(message) = ws.receive_message().await? {
//!         println!("Received: {:?}", message);
//!     }
//!     
//!     // Send actions manually
//!     ws.send_message(ClientMessage::Reveal { pos: Pos { x: 0, y: 0 } }).await?;
//!     
//!     ws.close().await?;
//!     Ok(())
//! }
//! ```

mod client;
mod game;
mod websocket;

pub use client::MinesweeperClient;
pub use game::{GameEvent, GameState, MinesweeperGame};
pub use websocket::MinesweeperWebSocket;

// Re-export common types for convenience
pub use minesweeper_common::{models::*, protocol::*};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
