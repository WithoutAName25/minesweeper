use minesweeper_client::{
    ClientMessage, GameParams, MinesweeperClient, MinesweeperWebSocket, Pos, ServerMessage,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create a client connecting to the server
    let client = MinesweeperClient::new("http://localhost:8000")?;

    // Create a new game
    let game_params = GameParams {
        width: 9,
        height: 9,
        bombs: 10,
    };

    let game_id = client.create_game(game_params).await?;
    println!("Created game with ID: {}", game_id);

    // Get the WebSocket URL for the game
    let ws_url = client.websocket_url(&game_id)?;
    println!("Connecting to WebSocket: {}", ws_url);

    // Connect to the game via WebSocket
    let mut ws = MinesweeperWebSocket::connect(&ws_url).await?;

    // Receive the initial game state
    if let Some(ServerMessage::Init {
        width,
        height,
        bombs,
        field,
    }) = ws.receive_message().await?
    {
        println!(
            "Received game initialization: {}x{} with {} bombs",
            width, height, bombs
        );

        // Print the initial field state
        for (y, row) in field.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                print!("[{},{}:{:?}] ", x, y, cell);
            }
            println!();
        }
    }

    // Send a reveal action
    let reveal_msg = ClientMessage::Reveal {
        pos: Pos { x: 0, y: 0 },
    };
    ws.send_message(reveal_msg).await?;
    println!("Sent reveal message for position (0, 0)");

    // Receive the response
    if let Some(message) = ws.receive_message().await? {
        match message {
            ServerMessage::Update { updates, won, lost } => {
                println!("Received update: {} cells updated", updates.len());
                for update in updates {
                    println!(
                        "  Cell ({}, {}) -> {:?}",
                        update.pos.x, update.pos.y, update.value
                    );
                }
                println!("Game state - Won: {}, Lost: {}", won, lost);
            }
            _ => println!("Received unexpected message: {:?}", message),
        }
    }

    // Send a flag action
    let flag_msg = ClientMessage::Flag {
        pos: Pos { x: 1, y: 1 },
    };
    ws.send_message(flag_msg).await?;
    println!("Sent flag message for position (1, 1)");

    // Receive the flag response
    if let Some(message) = ws.receive_message().await? {
        match message {
            ServerMessage::Update { updates, won, lost } => {
                println!("Received flag update: {} cells updated", updates.len());
                for update in updates {
                    println!(
                        "  Cell ({}, {}) -> {:?}",
                        update.pos.x, update.pos.y, update.value
                    );
                }
                println!("Game state - Won: {}, Lost: {}", won, lost);
            }
            _ => println!("Received unexpected message: {:?}", message),
        }
    }

    // Close the connection
    ws.close().await?;
    println!("Connection closed");

    Ok(())
}
