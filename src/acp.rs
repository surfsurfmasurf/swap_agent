use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;
use crate::price::PriceService;

#[derive(Debug, Serialize, Deserialize)]
pub struct JobStatus {
    pub job_id: String,
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct InvokeRequest {
    pub service_id: String,
    pub payment_tx: Option<String>,
    pub params: serde_json::Value,
    pub callback_url: Option<String>,
}

pub struct AcpService {
    jobs: RwLock<HashMap<String, JobStatus>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl AcpService {
    pub fn new() -> Self {
        Self { jobs: RwLock::new(HashMap::new()) }
    }

    pub fn get_offering_manifest(&self) -> serde_json::Value {
        let wallet = std::env::var("AGENT_WALLET_ADDRESS")
            .unwrap_or_else(|_| "YOUR_WALLET_ADDRESS_HERE".to_string());
        serde_json::json!({
            "agent_id": "swap-optimization-agent-v1",
            "agent_name": "Swap Optimization Agent",
            "description": "Real-time cross-chain swap optimizer: Solana (Jupiter) vs Base (OpenOcean)",
            "version": "0.1.0",
            "services": [
                {
                    "id": "price.compare",
                    "name": "Multi-chain Price Comparison",
                    "description": "Compare live swap prices across Solana and Base simultaneously",
                    "price_virtual": 0.1,
                    "input_schema": {
                        "token_in": "string (SOL/USDC/USDT/BONK/JUP/ETH/DAI)",
                        "token_out": "string",
                        "amount": "number"
                    }
                },
                {
                    "id": "swap.best_quote",
                    "name": "Best Swap Quote",
                    "description": "Returns the optimal swap route with output amount and reason",
                    "price_virtual": 0.5,
                    "input_schema": {
                        "token_in": "string",
                        "token_out": "string",
                        "amount_in": "number",
                        "max_slippage_pct": "number (optional, default 0.5)",
                        "preferred_chain": "string (optional: Solana/Base/auto)"
                    }
                }
            ],
            "payment": {
                "token": "VIRTUAL",
                "wallet_address": wallet,
                "accepted_chains": ["Base", "Solana"]
            }
        })
    }

    pub async fn handle_invoke(
        &self,
        req: InvokeRequest,
        price_service: &PriceService,
    ) -> Result<serde_json::Value> {
        let job_id = Uuid::new_v4().to_string();
        let now = now_ms();

        {
            let mut jobs = self.jobs.write().unwrap();
            jobs.insert(job_id.clone(), JobStatus {
                job_id: job_id.clone(),
                status: "running".to_string(),
                result: None,
                error: None,
                created_at: now,
                updated_at: now,
            });
        }

        let result = match req.service_id.as_str() {
            "price.compare" => {
                let token_in  = req.params["token_in"].as_str().unwrap_or("SOL");
                let token_out = req.params["token_out"].as_str().unwrap_or("USDC");
                let amount    = req.params["amount"].as_f64().unwrap_or(1.0);
                price_service.compare_prices(token_in, token_out, amount).await
                    .map(|r| serde_json::to_value(r).unwrap())
            }
            "swap.best_quote" => {
                let swap_req = crate::swap::SwapRequest {
                    token_in:         req.params["token_in"].as_str().unwrap_or("SOL").to_string(),
                    token_out:        req.params["token_out"].as_str().unwrap_or("USDC").to_string(),
                    amount_in:        req.params["amount_in"].as_f64().unwrap_or(1.0),
                    max_slippage_pct: req.params["max_slippage_pct"].as_f64(),
                    preferred_chain:  req.params["preferred_chain"].as_str().map(String::from),
                    wallet_address:   None,
                };
                price_service.get_best_quote(&swap_req).await
                    .map(|r| serde_json::to_value(r).unwrap())
            }
            unknown => Err(anyhow::anyhow!("Unknown service_id: {}", unknown)),
        };

        {
            let mut jobs = self.jobs.write().unwrap();
            if let Some(job) = jobs.get_mut(&job_id) {
                match &result {
                    Ok(val)  => { job.status = "done".to_string();   job.result = Some(val.clone()); }
                    Err(e)   => { job.status = "failed".to_string(); job.error  = Some(e.to_string()); }
                }
                job.updated_at = now_ms();
            }
        }

        match result {
            Ok(val) => Ok(serde_json::json!({ "job_id": job_id, "status": "done", "result": val })),
            Err(e) => Ok(serde_json::json!({ "job_id": job_id, "status": "failed", "error": e.to_string() })),
        }
    }

    pub fn get_job_status(&self, job_id: &str) -> serde_json::Value {
        let jobs = self.jobs.read().unwrap();
        match jobs.get(job_id) {
            Some(j) => serde_json::to_value(j).unwrap(),
            None    => serde_json::json!({ "error": "Job not found", "job_id": job_id }),
        }
    }
}
