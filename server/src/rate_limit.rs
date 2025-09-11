use std::{
    env,
    net::IpAddr,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use rocket::{State, http::Status};
use tracing::{debug, instrument, warn};

#[derive(Debug)]
pub struct TokenBucket {
    last_refill: Instant,
    tokens: u32,
    capacity: u32,
    refill_rate: u32,
    refill_interval: Duration,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: u32, refill_interval: Duration) -> Self {
        debug!(
            "Creating new token bucket: capacity={}, refill_rate={}, interval={}s",
            capacity,
            refill_rate,
            refill_interval.as_secs()
        );
        Self {
            last_refill: Instant::now(),
            tokens: capacity,
            capacity,
            refill_rate,
            refill_interval,
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens > 0 {
            self.tokens -= 1;
            debug!("Token consumed, remaining: {}", self.tokens);
            true
        } else {
            debug!("No tokens available for consumption");
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let intervals = elapsed.as_secs() / self.refill_interval.as_secs();

        if intervals > 0 {
            let old_tokens = self.tokens;
            let tokens_to_add = (intervals as u32) * self.refill_rate;
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
            self.last_refill = now;
            if self.tokens != old_tokens {
                debug!(
                    "Token bucket refilled: {} -> {} tokens",
                    old_tokens, self.tokens
                );
            }
        }
    }
}

pub type RateLimiter = DashMap<IpAddr, TokenBucket>;

pub fn create_rate_limiter() -> RateLimiter {
    DashMap::new()
}

#[instrument(level = "trace", skip(rate_limiter))]
pub fn check_rate_limit(rate_limiter: &State<RateLimiter>, ip: &IpAddr) -> Result<(), Status> {
    let capacity: u32 = env::var("RATE_LIMIT_GAMES_PER_MINUTE")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap_or(10);

    let refill_interval = Duration::from_secs(60); // 1 minute
    let refill_rate = capacity; // Refill to full capacity every minute

    let mut entry = rate_limiter
        .entry(*ip)
        .or_insert_with(|| TokenBucket::new(capacity, refill_rate, refill_interval));

    if entry.try_consume() {
        debug!("Rate limit check passed for {}", ip);
        Ok(())
    } else {
        warn!("Rate limit exceeded for {} - rejecting request", ip);
        Err(Status::TooManyRequests)
    }
}
