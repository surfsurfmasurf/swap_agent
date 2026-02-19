use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::swap::SwapRequest;

#[derive(Debug, Clone, PartialEq)]
pub enum Chain {
    Solana,
    Base,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PriceQuote {
    pub chain: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: f64,
    pub amount_out: f64,
    pub price_impact: f64,
    pub fee: f64,
    pub route: Vec<String>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceComparison {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: f64,
    pub solana_quote: Option<PriceQuote>,
    pub base_quote: Option<PriceQuote>,
    pub best_chain: String,
    pub best_output: f64,
    pub savings_pct: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BestQuoteResult {
    pub recommended: PriceQuote,
    pub alternatives: Vec<PriceQuote>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct JupiterQuoteResponse {
    #[serde(rename = "inAmount")]
    _in_amount: String,
    #[serde(rename = "outAmount")]
    out_amount: String,
    #[serde(rename = "priceImpactPct")]
    price_impact: String,
    #[serde(rename = "routePlan")]
    route_plan: Vec<JupiterRoutePlan>,
}

#[derive(Debug, Deserialize)]
struct JupiterRoutePlan {
    #[serde(rename = "swapInfo")]
    swap_info: JupiterSwapInfo,
}

#[derive(Debug, Deserialize)]
struct JupiterSwapInfo {
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OneInchQuoteResponse {
    #[serde(rename = "toAmount")]
    to_amount: String,
}

fn solana_token_address(symbol: &str) -> Option<&'static str> {
    match symbol.to_uppercase().as_str() {
        "SOL" | "WSOL" => Some("So11111111111111111111111111111111111111112"),
        "USDC"         => Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
        "USDT"         => Some("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"),
        "BONK"         => Some("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"),
        "JUP"          => Some("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN"),
        _              => None,
    }
}

fn base_token_address(symbol: &str) -> Option<&'static str> {
    match symbol.to_uppercase().as_str() {
        "ETH" | "WETH" => Some("0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE"),
        "USDC"         => Some("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"),
        "USDT"         => Some("0xfde4C96c8593536E31F229EA8f37b2ADa2699bb2"),
        "DAI"          => Some("0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb"),
        _              => None,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct PriceService {
    client: Client,
}

impl PriceService {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("SwapAgent/0.1.0")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    pub async fn get_price(
        &self,
        chain: Chain,
        token_in: &str,
        token_out: &str,
        amount: f64,
    ) -> Result<PriceQuote> {
        match chain {
            Chain::Solana => self.get_jupiter_quote(token_in, token_out, amount).await,
            Chain::Base   => self.get_oneinch_quote(token_in, token_out, amount).await,
        }
    }

    pub async fn compare_prices(
        &self,
        token_in: &str,
        token_out: &str,
        amount: f64,
    ) -> Result<PriceComparison> {
        let (sol_result, base_result) = tokio::join!(
            self.get_jupiter_quote(token_in, token_out, amount),
            self.get_oneinch_quote(token_in, token_out, amount),
        );

        let solana_quote = sol_result.ok();
        let base_quote   = base_result.ok();

        let (best_chain, best_output, savings_pct) = match (&solana_quote, &base_quote) {
            (Some(s), Some(b)) => {
                if s.amount_out >= b.amount_out {
                    ("Solana".to_string(), s.amount_out, (s.amount_out - b.amount_out) / b.amount_out * 100.0)
                } else {
                    ("Base".to_string(), b.amount_out, (b.amount_out - s.amount_out) / s.amount_out * 100.0)
                }
            }
            (Some(s), None) => ("Solana".to_string(), s.amount_out, 0.0),
            (None, Some(b)) => ("Base".to_string(), b.amount_out, 0.0),
            (None, None)    => return Err(anyhow::anyhow!("All chains failed")),
        };

        Ok(PriceComparison {
            token_in: token_in.to_string(),
            token_out: token_out.to_string(),
            amount_in: amount,
            solana_quote,
            base_quote,
            best_chain,
            best_output,
            savings_pct,
        })
    }

    pub async fn get_best_quote(&self, req: &SwapRequest) -> Result<BestQuoteResult> {
        let comparison = self.compare_prices(&req.token_in, &req.token_out, req.amount_in).await?;

        let (recommended, alternatives, reason) = match (&comparison.solana_quote, &comparison.base_quote) {
            (Some(s), Some(b)) => {
                if comparison.best_chain == "Solana" {
                    (s.clone(), vec![b.clone()], format!("Solana is {:.2}% better output", comparison.savings_pct))
                } else {
                    (b.clone(), vec![s.clone()], format!("Base is {:.2}% better output", comparison.savings_pct))
                }
            }
            (Some(s), None) => (s.clone(), vec![], "Only Solana available".to_string()),
            (None, Some(b)) => (b.clone(), vec![], "Only Base available".to_string()),
            _ => return Err(anyhow::anyhow!("No quotes available")),
        };

        Ok(BestQuoteResult { recommended, alternatives, reason })
    }

    async fn get_jupiter_quote(&self, token_in: &str, token_out: &str, amount: f64) -> Result<PriceQuote> {
        let input_mint  = solana_token_address(token_in).context(format!("Unknown Solana token: {}", token_in))?;
        let output_mint = solana_token_address(token_out).context(format!("Unknown Solana token: {}", token_out))?;

        let decimals_in  = if matches!(token_in.to_uppercase().as_str(),  "SOL" | "WSOL") { 9 } else { 6 };
        let decimals_out = if matches!(token_out.to_uppercase().as_str(), "SOL" | "WSOL") { 9 } else { 6 };
        let amount_raw   = (amount * 10f64.powi(decimals_in)) as u64;

        let url = format!(
            "https://quote-api.jup.ag/v6/quote?inputMint={}&outputMint={}&amount={}&slippageBps=50",
            input_mint, output_mint, amount_raw
        );

        let resp: JupiterQuoteResponse = self.client.get(&url).send().await?.json().await
            .context("Failed to parse Jupiter response")?;

        let amount_out   = resp.out_amount.parse::<f64>().unwrap_or(0.0) / 10f64.powi(decimals_out);
        let price_impact = resp.price_impact.parse::<f64>().unwrap_or(0.0);
        let route = resp.route_plan.iter()
            .map(|r| r.swap_info.label.clone().unwrap_or_else(|| "AMM".to_string()))
            .collect();

        Ok(PriceQuote {
            chain: "Solana".to_string(),
            token_in: token_in.to_string(),
            token_out: token_out.to_string(),
            amount_in: amount,
            amount_out,
            price_impact,
            fee: 0.25,
            route,
            timestamp_ms: now_ms(),
        })
    }

    async fn get_oneinch_quote(&self, token_in: &str, token_out: &str, amount: f64) -> Result<PriceQuote> {
        let src = base_token_address(token_in).context(format!("Unknown Base token: {}", token_in))?;
        let dst = base_token_address(token_out).context(format!("Unknown Base token: {}", token_out))?;

        let decimals_in  = if matches!(token_in.to_uppercase().as_str(),  "ETH" | "WETH") { 18 } else { 6 };
        let decimals_out = if matches!(token_out.to_uppercase().as_str(), "ETH" | "WETH") { 18 } else { 6 };
        let amount_raw   = format!("{:.0}", amount * 10f64.powi(decimals_in));

        let api_key = std::env::var("ONEINCH_API_KEY").unwrap_or_else(|_| "demo".to_string());
        let url = format!("https://api.1inch.dev/swap/v6.0/8453/quote?src={}&dst={}&amount={}", src, dst, amount_raw);

        let resp: OneInchQuoteResponse = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send().await?.json().await
            .context("Failed to parse 1inch response")?;

        let amount_out = resp.to_amount.parse::<f64>().unwrap_or(0.0) / 10f64.powi(decimals_out);

        Ok(PriceQuote {
            chain: "Base".to_string(),
            token_in: token_in.to_string(),
            token_out: token_out.to_string(),
            amount_in: amount,
            amount_out,
            price_impact: 0.0,
            fee: 0.3,
            route: vec!["1inch Fusion".to_string()],
            timestamp_ms: now_ms(),
        })
    }
}
