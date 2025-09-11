# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

- **Build**: `cargo build` (debug) or `cargo build --release` (optimized)
- **Run**: `cargo run` (starts the server on port 8000)
- **Format**: `cargo fmt` (applies rustfmt formatting)
- **Lint**: `cargo clippy -- -D warnings` (runs linter with warnings as errors)
- **Test**: `cargo test` (runs all tests)
- **Docker**: `docker build -t minesweeper-server .` and `docker run -p 8000:8000 minesweeper-server`

**IMPORTANT**: After every code change, run these commands to ensure code quality:
```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Architecture Overview

This is a **multiplayer minesweeper server and client** built with Rust using the Rocket web framework for the server and tokio-tungstenite for WebSocket client functionality. The server provides real-time multiplayer minesweeper games through WebSocket connections, and includes a comprehensive Rust client library.

### Server Components

- **server/main.rs**: Application entry point, sets up Rocket server with CORS, rate limiting, cleanup task, and routes
- **server/routes/mod.rs**: HTTP endpoints (`/create` for game creation) and WebSocket handler (`/ws`)
- **server/logic/mod.rs**: Game logic including bomb generation, cell revealing, game state management, and activity tracking
- **server/data/mod.rs**: Internal data structures (`Cell`, `Field`, `RevealedState`)
- **server/cors.rs**: CORS configuration with environment variable support
- **server/rate_limit.rs**: Rate limiting using token bucket algorithm per client IP
- **server/cleanup.rs**: Background task for automatic game cleanup based on activity timeouts

### Client Components

- **client/client.rs**: HTTP client for game creation and management
- **client/websocket.rs**: Thread-safe WebSocket client with MPSC channel pattern for concurrent read/write operations
- **client/game.rs**: High-level game client with background message listening, event emission, and local state management
- **common/**: Shared data models and protocol definitions used by both client and server

### Game Flow

1. **Game Creation**: POST `/create` with `GameParams` (width, height, bombs) returns game ID (rate limited per IP)
2. **WebSocket Connection**: GET `/ws?id=<game_id>` establishes real-time connection
3. **Game State**: Server broadcasts `ServerMessage::Init` on connection with full field state
4. **Player Actions**: Clients send `ClientMessage` (Reveal, Flag, Restart)
5. **State Updates**: Server broadcasts `ServerMessage::Update` with cell changes and win/loss status

### Client Usage

The client library provides both high-level and low-level interfaces:

#### High-Level Interface (Recommended)
```rust
use minesweeper_client::{GameEvent, GameParams, MinesweeperGame};

let mut game = MinesweeperGame::new("http://localhost:8000")?;

// Subscribe to real-time events
let mut events = game.subscribe_to_events();
tokio::spawn(async move {
    while let Some(event) = events.recv().await {
        match event {
            GameEvent::BoardUpdated { changed_positions } => { /* handle updates */ }
            GameEvent::GameStatusChanged { won, lost } => { /* handle win/loss */ }
            // ... other events
        }
    }
});

// Start game with optional parameters (defaults to 9x9 with 10 bombs)
let params = GameParams::new(); // or GameParams { width: 16, height: 16, bombs: 40 }
game.start_game(params).await?;

// Make moves (non-blocking)
game.reveal(0, 0).await?;
game.flag(1, 1).await?;

// Get current state
if let Some(state) = game.get_state().await {
    println!("Game over: {}", state.is_game_over());
}
```

#### Low-Level Interface
```rust
use minesweeper_client::{MinesweeperClient, MinesweeperWebSocket, ClientMessage};

let client = MinesweeperClient::new("http://localhost:8000")?;
let game_id = client.create_game(GameParams::new()).await?;

let mut ws = MinesweeperWebSocket::connect(&client.websocket_url(&game_id)?).await?;
ws.send_message(ClientMessage::Reveal { pos: Pos { x: 0, y: 0 } }).await?;
```

### Rate Limiting

- **Token Bucket Algorithm**: Each client IP gets a separate token bucket
- **Default Limit**: 10 games per minute per IP address
- **Response**: Returns HTTP 429 (Too Many Requests) when limit exceeded
- **IP Detection**: Uses `X-Forwarded-For`, `X-Real-IP` headers or connection IP

### Game Cleanup

- **Automatic Cleanup**: Background task runs every 60 seconds by default
- **Two Timeout Types**:
  - **Inactive Games**: Games with no WebSocket connections are cleaned up after 5 minutes
  - **Active Games**: Games with connections but no activity are cleaned up after 1 hour
- **Activity Tracking**: Last activity updated on game actions (reveal, flag, restart, connection events)

### Key Data Structures

- **Games**: `DashMap<String, Arc<Mutex<Game>>>` - Thread-safe game storage
- **Game**: Contains `Field`, WebSocket connections (`HashMap<Uuid, SplitSink>`), and activity timestamps
- **Field**: Game state with cells, dimensions, bomb count, and completion status
- **Cell**: Internal cell with bomb flag, adjacent count, and revealed state

### WebSocket Protocol

- **Client Messages**: `{"action": "reveal|flag|restart", "pos": {"x": 0, "y": 0}}`
- **Server Messages**: 
  - `{"type": "init", "width": 10, "height": 10, "bombs": 10, "field": [[...]]}`
  - `{"type": "update", "updates": [...], "won": false, "lost": false}`

### Environment Configuration

- **CORS_ALLOWED_ORIGINS**: Comma-separated list of allowed origins (default: `http://localhost:5173`)
- **RATE_LIMIT_GAMES_PER_MINUTE**: Games per minute per IP address (default: `10`)
- **CLEANUP_INTERVAL_SECONDS**: How often to run cleanup task (default: `60`)
- **INACTIVE_GAME_TIMEOUT_SECONDS**: Timeout for games with no active connections (default: `300` - 5 minutes)
- **ACTIVE_GAME_TIMEOUT_SECONDS**: Timeout for games with active connections but no activity (default: `3600` - 1 hour)
- **RUST_LOG**: Logging level (default: `info` in Docker)
- **ROCKET_ENV**: Environment (`prod` in Docker)
- **ROCKET_ADDRESS**: Bind address (`0.0.0.0` in Docker)
- **ROCKET_PORT**: Port number (`8000` in Docker)

### Client Architecture Features

- **Thread-Safe WebSocket**: Uses MPSC channel pattern to prevent read/write deadlocks
- **Background Message Processing**: Automatic WebSocket message handling in background tasks
- **Event-Driven Updates**: Real-time field change notifications with exact position data
- **Optional Game Parameters**: Server-side defaults (9x9 with 10 bombs) with serde support
- **Concurrent Operations**: Non-blocking game actions (reveal, flag, restart) while listening for updates
- **Automatic State Management**: Local game state synchronization with server

### Key Data Structures

#### Server
- **Games**: `DashMap<String, Arc<Mutex<Game>>>` - Thread-safe game storage
- **Game**: Contains `Field`, WebSocket connections (`HashMap<Uuid, SplitSink>`), and activity timestamps
- **Field**: Game state with cells, dimensions, bomb count, and completion status
- **Cell**: Internal cell with bomb flag, adjacent count, and revealed state

#### Client
- **GameEvent**: Enum for real-time events (BoardUpdated, GameStatusChanged, GameInitialized, ConnectionLost)
- **GameState**: Local representation of the game board with utility methods
- **MinesweeperGame**: High-level client with event subscription and background processing
- **MinesweeperWebSocket**: Thread-safe WebSocket wrapper with internal MPSC channel

### Dependencies

#### Server
- **Rocket**: Web framework with WebSocket support (`rocket_ws`)
- **DashMap**: Concurrent hash map for game storage
- **Tokio**: Async runtime
- **Serde**: JSON serialization
- **UUID/NanoID**: Unique identifiers
- **Tracing**: Structured logging

#### Client
- **tokio-tungstenite**: WebSocket client implementation
- **reqwest**: HTTP client for REST API calls
- **futures-util**: Stream/Sink utilities for WebSocket handling
- **Tokio**: Async runtime and synchronization primitives
- **Serde**: JSON serialization
- **Tracing**: Structured logging

### Testing & CI

The project uses GitHub Actions CI with:
- Rust formatting checks (`cargo fmt -- --check`)
- Clippy linting (`cargo clippy -- -D warnings`)
- Test execution (`cargo test`)
- Docker image building and publishing on main branch