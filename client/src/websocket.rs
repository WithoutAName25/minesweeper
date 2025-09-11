use futures_util::{SinkExt, StreamExt, stream::SplitStream};
use minesweeper_common::protocol::{ClientMessage, ServerMessage};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use crate::Result;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsReader = SplitStream<WsStream>;

/// WebSocket client for real-time minesweeper gameplay
pub struct MinesweeperWebSocket {
    sender: mpsc::UnboundedSender<ClientMessage>,
    reader: WsReader,
    writer_task: JoinHandle<()>,
}

impl MinesweeperWebSocket {
    /// Connect to a minesweeper game via WebSocket
    pub async fn connect(url: &str) -> Result<Self> {
        info!("Connecting to WebSocket: {}", url);

        let (ws_stream, _) = connect_async(url).await?;
        info!("WebSocket connected successfully");

        let (writer, reader) = ws_stream.split();

        // Create MPSC channel for sending messages
        let (sender, mut receiver) = mpsc::unbounded_channel::<ClientMessage>();

        // Spawn writer task that handles all outgoing messages
        let writer_task = tokio::spawn(async move {
            let mut writer = writer;
            while let Some(message) = receiver.recv().await {
                let json = match serde_json::to_string(&message) {
                    Ok(json) => json,
                    Err(e) => {
                        warn!("Failed to serialize message: {}", e);
                        continue;
                    }
                };

                debug!("Sending message: {}", json);
                if let Err(e) = writer.send(Message::Text(json.into())).await {
                    warn!("Failed to send WebSocket message: {}", e);
                    break;
                }
            }

            // Close the writer when done
            let _ = writer.close().await;
        });

        Ok(Self {
            sender,
            reader,
            writer_task,
        })
    }

    /// Get a cloneable sender for sending messages
    pub fn get_sender(&self) -> mpsc::UnboundedSender<ClientMessage> {
        self.sender.clone()
    }

    /// Send a client message to the server
    pub async fn send_message(&self, message: ClientMessage) -> Result<()> {
        self.sender
            .send(message)
            .map_err(|_| "WebSocket sender channel closed")?;
        Ok(())
    }

    /// Receive the next server message
    /// Returns None if the connection is closed
    pub async fn receive_message(&mut self) -> Result<Option<ServerMessage>> {
        if let Some(msg) = self.reader.next().await {
            match msg? {
                Message::Text(text) => {
                    debug!("Received message: {}", text);
                    let server_message: ServerMessage = serde_json::from_str(&text)?;
                    Ok(Some(server_message))
                }
                Message::Close(_) => {
                    info!("WebSocket connection closed");
                    Ok(None)
                }
                _ => {
                    // Ignore ping/pong and binary messages, try again
                    Box::pin(self.receive_message()).await
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Close the WebSocket connection
    pub async fn close(self) -> Result<()> {
        // Drop the sender to signal the writer task to close
        drop(self.sender);

        // Wait for the writer task to complete
        let _ = self.writer_task.await;

        Ok(())
    }
}
