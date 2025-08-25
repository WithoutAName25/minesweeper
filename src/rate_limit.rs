use std::{
    env,
    net::IpAddr,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use rocket::{
    State,
    http::Status,
    request::{self, FromRequest, Request},
};

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
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let intervals = elapsed.as_secs() / self.refill_interval.as_secs();

        if intervals > 0 {
            let tokens_to_add = (intervals as u32) * self.refill_rate;
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
            self.last_refill = now;
        }
    }
}

pub type RateLimiter = DashMap<IpAddr, TokenBucket>;

pub fn create_rate_limiter() -> RateLimiter {
    DashMap::new()
}

pub struct ClientIp(pub IpAddr);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientIp {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let ip = req
            .headers()
            .get_one("X-Forwarded-For")
            .and_then(|header| header.split(',').next())
            .and_then(|ip| ip.trim().parse().ok())
            .or_else(|| {
                req.headers()
                    .get_one("X-Real-IP")
                    .and_then(|ip| ip.parse().ok())
            })
            .or_else(|| req.client_ip())
            .unwrap_or_else(|| "127.0.0.1".parse().unwrap());

        request::Outcome::Success(ClientIp(ip))
    }
}

pub fn check_rate_limit(
    rate_limiter: &State<RateLimiter>,
    client_ip: &ClientIp,
) -> Result<(), Status> {
    let capacity: u32 = env::var("RATE_LIMIT_GAMES_PER_MINUTE")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap_or(10);

    let refill_interval = Duration::from_secs(60); // 1 minute
    let refill_rate = capacity; // Refill to full capacity every minute

    let mut entry = rate_limiter
        .entry(client_ip.0)
        .or_insert_with(|| TokenBucket::new(capacity, refill_rate, refill_interval));

    if entry.try_consume() {
        Ok(())
    } else {
        Err(Status::TooManyRequests)
    }
}
