use minesweeper_common::models::{CreateResponse, GameParams};
use reqwest::Client;
use url::Url;

use crate::Result;

/// HTTP client for minesweeper server API
pub struct MinesweeperClient {
    client: Client,
    base_url: Url,
}

impl MinesweeperClient {
    /// Create a new client connecting to the specified server URL
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url)?;
        let client = Client::new();

        Ok(Self { client, base_url })
    }

    /// Create a new game with the specified parameters
    /// Returns the game ID that can be used to connect via WebSocket
    pub async fn create_game(&self, params: GameParams) -> Result<String> {
        let create_url = self.base_url.join("/create")?;

        let response = self.client.post(create_url).json(&params).send().await?;

        if !response.status().is_success() {
            return Err(format!("Failed to create game: {}", response.status()).into());
        }

        let create_response: CreateResponse = response.json().await?;
        Ok(create_response.id)
    }

    /// Get the WebSocket URL for a game
    pub fn websocket_url(&self, game_id: &str) -> Result<String> {
        let mut ws_url = self.base_url.clone();
        ws_url
            .set_scheme(match self.base_url.scheme() {
                "https" => "wss",
                _ => "ws",
            })
            .map_err(|_| "Failed to set WebSocket scheme")?;
        ws_url.set_path("/ws");
        ws_url.set_query(Some(&format!("id={}", game_id)));

        Ok(ws_url.to_string())
    }
}
