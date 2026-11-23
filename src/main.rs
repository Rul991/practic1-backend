mod config; mod dto; mod error; mod extractors; mod handlers; mod jwt; mod password; mod state;

use std::sync::Arc;
use axum::{
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{config::Config, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Arc::new(Config::from_env()?);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState { db: pool, config: config.clone() };

    let app = Router::new()
        // auth
        .route("/api/v1/auth/register", post(handlers::register))
        .route("/api/v1/auth/login",    post(handlers::login))
        .route("/api/v1/auth/me",       get(handlers::me))
        // surveys — owner
        .route("/api/v1/surveys",
            get(handlers::list_own_surveys).post(handlers::create_survey))
        .route("/api/v1/surveys/{id}",
            get(handlers::get_own_survey)
                .patch(handlers::update_survey)
                .delete(handlers::delete_survey))
        .route("/api/v1/surveys/{id}/results",   get(handlers::get_results))
        .route("/api/v1/surveys/{id}/responses", get(handlers::list_responses))
        // public
        .route("/api/v1/public/surveys/{link}",           get(handlers::get_public_survey))
        .route("/api/v1/public/surveys/{link}/responses", post(handlers::submit_response))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}