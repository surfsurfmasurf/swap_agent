use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Price fetch failed ({chain}): {message}")]
    PriceFetch { chain: String, message: String },

    #[error("Unsupported token: {symbol} on {chain}")]
    UnsupportedToken { symbol: String, chain: String },

    #[error("Slippage exceeded: expected {expected:.2}%, actual {actual:.2}%")]
    SlippageExceeded { expected: f64, actual: f64 },

    #[error("Payment verification failed: {0}")]
    PaymentVerification(String),

    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}
