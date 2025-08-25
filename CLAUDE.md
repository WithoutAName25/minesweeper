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

This is a **multiplayer minesweeper server** built with Rust using the Rocket web framework. The server provides real-time multiplayer minesweeper games through WebSocket connections.

### Core Components

- **main.rs**: Application entry point, sets up Rocket server with CORS, rate limiting, and routes
- **routes/mod.rs**: HTTP endpoints (`/create` for game creation) and WebSocket handler (`/ws`)
- **logic/mod.rs**: Game logic including bomb generation, cell revealing, and game state management
- **data/mod.rs**: Internal data structures (`Cell`, `Field`, `RevealedState`)
- **model/**: API data models for client-server communication
- **cors.rs**: CORS configuration with environment variable support
- **rate_limit.rs**: Rate limiting using token bucket algorithm per client IP

### Game Flow

1. **Game Creation**: POST `/create` with `GameParams` (width, height, bombs) returns game ID (rate limited per IP)
2. **WebSocket Connection**: GET `/ws?id=<game_id>` establishes real-time connection
3. **Game State**: Server broadcasts `ServerMessage::Init` on connection with full field state
4. **Player Actions**: Clients send `ClientMessage` (Reveal, Flag, Restart)
5. **State Updates**: Server broadcasts `ServerMessage::Update` with cell changes and win/loss status

### Rate Limiting

- **Token Bucket Algorithm**: Each client IP gets a separate token bucket
- **Default Limit**: 10 games per minute per IP address
- **Response**: Returns HTTP 429 (Too Many Requests) when limit exceeded
- **IP Detection**: Uses `X-Forwarded-For`, `X-Real-IP` headers or connection IP

### Key Data Structures

- **Games**: `DashMap<String, Arc<Mutex<Game>>>` - Thread-safe game storage
- **Game**: Contains `Field` and WebSocket connections (`HashMap<Uuid, SplitSink>`)
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
- **RUST_LOG**: Logging level (default: `info` in Docker)
- **ROCKET_ENV**: Environment (`prod` in Docker)
- **ROCKET_ADDRESS**: Bind address (`0.0.0.0` in Docker)
- **ROCKET_PORT**: Port number (`8000` in Docker)

### Dependencies

- **Rocket**: Web framework with WebSocket support (`rocket_ws`)
- **DashMap**: Concurrent hash map for game storage
- **Tokio**: Async runtime
- **Serde**: JSON serialization
- **UUID/NanoID**: Unique identifiers
- **Tracing**: Structured logging

### Testing & CI

The project uses GitHub Actions CI with:
- Rust formatting checks (`cargo fmt -- --check`)
- Clippy linting (`cargo clippy -- -D warnings`)
- Test execution (`cargo test`)
- Docker image building and publishing on main branch