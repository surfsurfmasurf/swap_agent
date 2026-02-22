mod price;
mod swap;
mod acp;
mod error;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use acp::{AcpService, InvokeRequest};
use price::{Chain, PriceService};
use swap::SwapRequest;

pub struct AppState {
    pub price_service: PriceService,
    pub acp_service: AcpService,
}

#[derive(Deserialize)]
struct PriceQuery {
    token_in: String,
    token_out: String,
    amount: Option<f64>,
}

#[derive(Deserialize)]
struct SwapQuoteQuery {
    token_in: String,
    token_out: String,
    amount_in: f64,
    max_slippage_pct: Option<f64>,
    preferred_chain: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "swap_agent=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Swap Optimization Agent...");

    let state = Arc::new(AppState {
        price_service: PriceService::new(),
        acp_service: AcpService::new(),
    });

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let app = Router::new()
        .route("/health",                   get(health))
        .route("/api/v1/price/solana",      get(price_solana))
        .route("/api/v1/price/base",        get(price_base))
        .route("/api/v1/price/compare",     get(price_compare))
        .route("/api/v1/swap/quote",        get(swap_quote_get).post(swap_quote))
        .route("/api/v1/swap/execute",      post(swap_execute))
        .route("/acp/offering",             get(acp_offering))
        .route("/acp/invoke",               post(acp_invoke))
        .route("/acp/status/:job_id",       get(acp_status))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "agent": "swap-optimization-agent", "version": "0.1.0" }))
}

async fn price_solana(
    State(s): State<Arc<AppState>>,
    Query(q): Query<PriceQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    s.price_service.get_price(Chain::Solana, &q.token_in, &q.token_out, q.amount.unwrap_or(1.0))
        .await
        .map(|r| Json(serde_json::to_value(r).unwrap()))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn price_base(
    State(s): State<Arc<AppState>>,
    Query(q): Query<PriceQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    s.price_service.get_price(Chain::Base, &q.token_in, &q.token_out, q.amount.unwrap_or(1.0))
        .await
        .map(|r| Json(serde_json::to_value(r).unwrap()))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn price_compare(
    State(s): State<Arc<AppState>>,
    Query(q): Query<PriceQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    s.price_service.compare_prices(&q.token_in, &q.token_out, q.amount.unwrap_or(1.0))
        .await
        .map(|r| Json(serde_json::to_value(r).unwrap()))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn swap_quote_get(
    State(s): State<Arc<AppState>>,
    Query(q): Query<SwapQuoteQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let req = SwapRequest {
        token_in: q.token_in,
        token_out: q.token_out,
        amount_in: q.amount_in,
        max_slippage_pct: q.max_slippage_pct,
        preferred_chain: q.preferred_chain,
        wallet_address: None,
    };
    s.price_service.get_best_quote(&req)
        .await
        .map(|r| Json(serde_json::to_value(r).unwrap()))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn swap_quote(
    State(s): State<Arc<AppState>>,
    Json(req): Json<SwapRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    s.price_service.get_best_quote(&req)
        .await
        .map(|r| Json(serde_json::to_value(r).unwrap()))
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn swap_execute(Json(req): Json<SwapRequest>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "simulated",
        "message": "Live execution requires wallet integration",
        "request": req
    }))
}

async fn acp_offering(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(s.acp_service.get_offering_manifest())
}

async fn acp_invoke(
    State(s): State<Arc<AppState>>,
    Json(req): Json<InvokeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    s.acp_service.handle_invoke(req, &s.price_service)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn acp_status(
    State(s): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Json<serde_json::Value> {
    Json(s.acp_service.get_job_status(&job_id))
}
