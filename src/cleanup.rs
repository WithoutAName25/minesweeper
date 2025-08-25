use std::{env, time::Duration};

use tokio::time;
use tracing::{debug, info};

use crate::logic::Games;

pub async fn start_cleanup_task(games: Games) {
    let cleanup_interval_secs: u64 = env::var("CLEANUP_INTERVAL_SECONDS")
        .unwrap_or_else(|_| "60".to_string())
        .parse()
        .unwrap_or(60);

    let inactive_timeout_secs: u64 = env::var("INACTIVE_GAME_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "600".to_string())
        .parse()
        .unwrap_or(600);

    let active_timeout_secs: u64 = env::var("ACTIVE_GAME_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "86400".to_string())
        .parse()
        .unwrap_or(86400);

    let mut interval = time::interval(Duration::from_secs(cleanup_interval_secs));

    info!(
        "Started game cleanup task: checking every {}s, inactive timeout: {}s, active timeout: {}s",
        cleanup_interval_secs, inactive_timeout_secs, active_timeout_secs
    );

    loop {
        interval.tick().await;
        cleanup_games(&games, inactive_timeout_secs, active_timeout_secs).await;
    }
}

async fn cleanup_games(games: &Games, inactive_timeout_secs: u64, active_timeout_secs: u64) {
    let mut games_to_remove = Vec::new();

    // First pass: identify games to remove
    for entry in games.iter() {
        let game_id = entry.key();
        let game = entry.value();

        // Try to lock the game, skip if we can't (probably in use)
        if let Ok(game_guard) = game.try_lock()
            && game_guard.should_cleanup(inactive_timeout_secs, active_timeout_secs)
        {
            games_to_remove.push(game_id.clone());
        }
    }

    // Second pass: remove identified games
    let removed_count = games_to_remove.len();
    for game_id in games_to_remove {
        games.remove(&game_id);
        debug!("Cleaned up game: {}", game_id);
    }

    if removed_count > 0 {
        info!("Cleaned up {} inactive games", removed_count);
    }
}
