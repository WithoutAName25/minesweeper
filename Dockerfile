FROM rust:latest AS builder

WORKDIR /app

COPY Cargo.toml ./

# Copy crate manifests for dependency caching
COPY common/Cargo.toml common/
COPY server/Cargo.toml server/
COPY client/Cargo.toml client/

# Create dummy source files for dependency caching
RUN mkdir -p common/src && echo "" > common/src/lib.rs
RUN mkdir -p server/src && echo "fn main() {}" > server/src/main.rs
RUN mkdir -p client/src && echo "" > client/src/lib.rs

# Build dependencies (this will be cached)
RUN cargo build --release --bin minesweeper-server
RUN rm -rf common/src server/src client/src

# Copy actual source code
COPY common/src common/src
COPY server/src server/src
COPY client/src client/src

# Build the server binary
RUN touch common/src/lib.rs server/src/main.rs client/src/lib.rs && cargo build --release --bin minesweeper-server

# Runtime stage
FROM debian:bookworm-slim

RUN useradd -r -s /bin/false -m -d /app appuser

WORKDIR /app

COPY --from=builder /app/target/release/minesweeper-server ./app

RUN chown -R appuser:appuser /app
USER appuser

ENV RUST_LOG=info
ENV ROCKET_ENV=prod
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8000

EXPOSE 8000

CMD ["./app"]
