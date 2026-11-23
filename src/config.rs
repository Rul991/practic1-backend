use anyhow::Context;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret:   String,
    pub jwt_ttl_secs: i64,
    pub bind_addr:    String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL")?,
            jwt_secret:   std::env::var("JWT_SECRET").context("JWT_SECRET")?,
            jwt_ttl_secs: std::env::var("JWT_TTL_SECS").ok()
                .and_then(|v| v.parse().ok()).unwrap_or(86_400),
            bind_addr:    std::env::var("BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3000".into()),
        })
    }
}