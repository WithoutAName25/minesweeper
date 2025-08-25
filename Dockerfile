FROM rust:latest AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir -p src/ && echo "fn main() {}" > src/main.rs

# Build dependencies (this will be cached)
RUN cargo build --release
RUN rm -r src/*

# Copy source code
COPY src src

# Build the application
# Touch main.rs to ensure it's rebuilt
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN useradd -r -s /bin/false -m -d /app appuser

WORKDIR /app

COPY --from=builder /app/target/release/minesweeper_server ./app

RUN chown -R appuser:appuser /app
USER appuser

ENV RUST_LOG=info
ENV ROCKET_ENV=prod
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8000

EXPOSE 8000

CMD ["./app"]
