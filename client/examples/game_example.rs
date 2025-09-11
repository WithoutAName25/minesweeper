use minesweeper_client::{GameEvent, GameParams, MinesweeperGame};
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create a high-level game client
    let mut game = MinesweeperGame::new("http://localhost:8000")?;

    // Subscribe to game events for background listening
    let mut event_receiver = game.subscribe_to_events();

    // Spawn background task to handle events
    let event_handler = tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            match event {
                GameEvent::GameInitialized {
                    width,
                    height,
                    bombs,
                } => {
                    println!(
                        "🎮 Game initialized: {}x{} with {} bombs",
                        width, height, bombs
                    );
                }
                GameEvent::BoardUpdated { changed_positions } => {
                    println!(
                        "📋 {} cells updated: {:?}",
                        changed_positions.len(),
                        changed_positions
                    );
                }
                GameEvent::GameStatusChanged { won, lost } => {
                    if won {
                        println!("🎉 You won!");
                    } else if lost {
                        println!("💣 Game over!");
                    }
                }
                GameEvent::ConnectionLost => {
                    println!("🔌 Connection lost!");
                    break;
                }
            }
        }
    });

    // Start a new 8x8 game with 10 bombs
    let params = GameParams {
        width: 8,
        height: 8,
        bombs: 10,
    };

    game.start_game(params).await?;
    println!("Game started! Game ID: {}", game.get_game_id().unwrap());

    // Give time for initialization event
    sleep(Duration::from_millis(100)).await;

    // Display initial board state
    if let Some(state) = game.get_state().await {
        println!(
            "\nInitial board ({}x{} with {} bombs):",
            state.width, state.height, state.bombs
        );
        display_board(&state);

        let cell_counts = state.count_cells();
        println!("Cell counts: {:?}", cell_counts);
    }

    // Make some moves
    println!("\n=== Making some moves ===");

    // Reveal the corner cell (0, 0)
    println!("Revealing cell (0, 0)...");
    game.reveal(0, 0).await?;
    sleep(Duration::from_millis(100)).await;

    if let Some(state) = game.get_state().await {
        display_board(&state);
        if state.is_game_over() {
            println!("Game over! Won: {}", state.is_won());
        }
    }

    // Try to flag a cell (1, 1)
    println!("\nFlagging cell (1, 1)...");
    game.flag(1, 1).await?;
    sleep(Duration::from_millis(100)).await;

    if let Some(state) = game.get_state().await {
        display_board(&state);
    }

    // Try to reveal another cell (2, 2)
    println!("\nRevealing cell (2, 2)...");
    game.reveal(2, 2).await?;
    sleep(Duration::from_millis(100)).await;

    if let Some(state) = game.get_state().await {
        display_board(&state);
        if state.is_game_over() {
            println!("Game over! Won: {}", state.is_won());
        }
    }

    // Flag the same cell again (should unflag it)
    println!("\nUnflagging cell (1, 1)...");
    game.flag(1, 1).await?;
    sleep(Duration::from_millis(100)).await;

    if let Some(state) = game.get_state().await {
        display_board(&state);
        let cell_counts = state.count_cells();
        println!("Final cell counts: {:?}", cell_counts);
    }

    // Disconnect from the game
    game.disconnect().await?;
    println!("\nDisconnected from game");

    // Clean up event handler
    event_handler.abort();
    let _ = event_handler.await;

    Ok(())
}

fn display_board(state: &minesweeper_client::GameState) {
    println!("Board state:");
    for (y, row) in state.board.iter().enumerate() {
        print!("  ");
        for (_x, cell) in row.iter().enumerate() {
            let symbol = match cell {
                minesweeper_client::Cell::Hidden => "·",
                minesweeper_client::Cell::Marked => "?",
                minesweeper_client::Cell::Flagged => "F",
                minesweeper_client::Cell::Revealed { adjacent } => {
                    match adjacent {
                        0 => " ",
                        n => {
                            // Use a simple character representation for numbers
                            match n {
                                1 => "1",
                                2 => "2",
                                3 => "3",
                                4 => "4",
                                5 => "5",
                                6 => "6",
                                7 => "7",
                                8 => "8",
                                _ => "X",
                            }
                        }
                    }
                }
                minesweeper_client::Cell::Bomb => "💣",
            };
            print!("{:2}", symbol);
        }
        println!("  {}", y);
    }

    // Print x coordinates
    print!("  ");
    for x in 0..state.width {
        print!("{:2}", x);
    }
    println!();
}
