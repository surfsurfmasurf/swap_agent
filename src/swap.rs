use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwapRequest {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: f64,
    pub max_slippage_pct: Option<f64>,
    pub preferred_chain: Option<String>,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwapExecutionResult {
    pub status: SwapStatus,
    pub tx_hash: Option<String>,
    pub chain: String,
    pub amount_in: f64,
    pub amount_out: f64,
    pub actual_price_impact: f64,
    pub gas_used: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapStatus {
    Pending,
    Confirmed,
    Failed,
    Simulated,
}
