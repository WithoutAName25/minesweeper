use rocket::http::Method;
use rocket_cors::{AllowedOrigins, CorsOptions};
use std::env;

pub fn create_cors() -> rocket_cors::Cors {
    let allowed_origins_env =
        env::var("CORS_ALLOWED_ORIGINS").unwrap_or_else(|_| "http://localhost:5173".to_string());

    let allowed_origins: Vec<String> = allowed_origins_env
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let allowed_origins = AllowedOrigins::some_exact(&allowed_origins);

    CorsOptions {
        allowed_origins,
        allowed_methods: vec![
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Delete,
            Method::Patch,
            Method::Options,
        ]
        .into_iter()
        .map(|m| m.into())
        .collect(),
        allowed_headers: rocket_cors::AllowedHeaders::some(&[
            "Authorization",
            "Accept",
            "Content-Type",
            "X-Requested-With",
        ]),
        allow_credentials: true,
        ..Default::default()
    }
    .to_cors()
    .expect("Failed to create CORS configuration")
}
