# Swap Optimization Agent

A cross-chain swap optimization HTTP API written in Rust. It queries live prices from **Jupiter** (Solana) and **1inch** (Base) in parallel, compares them, and recommends the best route. It also exposes an **ACP (Agent Commerce Protocol)** interface so AI agents can invoke its services programmatically.

---

## Table of Contents

- [How It Works](#how-it-works)
- [Architecture](#architecture)
- [Project Structure](#project-structure)
- [Supported Tokens](#supported-tokens)
- [Configuration](#configuration)
- [Building & Running](#building--running)
  - [From Source](#from-source)
  - [As a systemd Service](#as-a-systemd-service)
- [API Reference](#api-reference)
  - [Health](#get-health)
  - [Solana Price](#get-apiv1pricesolana)
  - [Base Price](#get-apiv1pricebase)
  - [Cross-chain Compare](#get-apiv1pricecompare)
  - [Best Swap Quote](#post-apiv1swapquote)
  - [Swap Execute](#post-apiv1swapexecute)
  - [ACP Offering](#get-acpoffering)
  - [ACP Invoke](#post-acpinvoke)
  - [ACP Job Status](#get-acpstatusjob_id)
- [Testing](#testing)

---

## How It Works

```
User / AI Agent
      │
      ▼
┌─────────────────────────────┐
│   Swap Optimization Agent   │  (Axum HTTP, port 8080)
│                             │
│  ┌──────────────────────┐  │
│  │    PriceService      │  │
│  │  ┌────────────────┐  │  │
│  │  │ Jupiter (v6)   │──┼──┼──► quote-api.jup.ag  (Solana)
│  │  ├────────────────┤  │  │
│  │  │ 1inch (v6.0)   │──┼──┼──► api.1inch.dev     (Base)
│  │  └────────────────┘  │  │
│  │  compare → best route │  │
│  └──────────────────────┘  │
│                             │
│  ┌──────────────────────┐  │
│  │    AcpService        │  │
│  │  in-memory job store │  │
│  │  price.compare       │  │
│  │  swap.best_quote     │  │
│  └──────────────────────┘  │
└─────────────────────────────┘
```

1. **Price queries** — when a price endpoint is called, the agent resolves token symbols to their on-chain addresses, converts the human-readable amount to the token's base unit, and calls the DEX aggregator API.
2. **Cross-chain comparison** — both chains are queried concurrently with `tokio::join!`. The one returning more `amount_out` wins. The savings percentage is also calculated.
3. **Best quote** — wraps the comparison result into a recommended + alternatives structure with a human-readable reason string.
4. **ACP** — the agent advertises its services at `/acp/offering`. Callers `POST /acp/invoke` with a `service_id` and params. The result is returned synchronously and also stored in the in-memory job store for later polling via `/acp/status/:job_id`.

---

## Architecture

```
src/
├── main.rs    — HTTP server bootstrap, route definitions, handler functions
├── price.rs   — PriceService: Jupiter + 1inch API clients, quote logic
├── swap.rs    — SwapRequest / SwapExecutionResult data types
├── acp.rs     — AcpService: offering manifest, invoke handler, job store
└── error.rs   — AgentError enum (thiserror)
```

**Key dependencies:**

| Crate | Purpose |
|---|---|
| `axum 0.7` | Async HTTP framework |
| `tokio 1` (full) | Async runtime |
| `reqwest 0.11` | Outbound HTTP to DEX APIs (rustls TLS) |
| `serde / serde_json` | JSON serialization |
| `tower-http 0.5` | CORS + request tracing middleware |
| `uuid 1` | Job ID generation for ACP |
| `tracing / tracing-subscriber` | Structured logging with `RUST_LOG` filter |
| `anyhow / thiserror` | Error handling |

---

## Project Structure

```
swap_agent/
├── Cargo.toml        # Package manifest & dependencies
├── Cargo.lock        # Locked dependency versions
├── .env              # Environment configuration (not committed)
├── .gitignore
├── agent.log         # Runtime log file (not committed)
└── src/
    ├── main.rs
    ├── price.rs
    ├── swap.rs
    ├── acp.rs
    └── error.rs
```

---

## Supported Tokens

### Solana (Jupiter)

| Symbol | Mint Address |
|---|---|
| SOL / WSOL | `So11111111111111111111111111111111111111112` |
| USDC | `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v` |
| USDT | `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB` |
| BONK | `DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263` |
| JUP | `JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN` |

### Base (1inch)

| Symbol | Contract Address |
|---|---|
| ETH / WETH | `0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE` |
| USDC | `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913` |
| USDT | `0xfde4C96c8593536E31F229EA8f37b2ADa2699bb2` |
| DAI | `0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb` |

Token decimals are handled automatically (9 for SOL/WSOL, 18 for ETH/WETH, 6 for all stablecoins).

---

## Configuration

Create a `.env` file in the project root (next to `Cargo.toml`):

```bash
# Network interface and port to bind (default: 0.0.0.0:8080)
BIND_ADDR=0.0.0.0:8080

# 1inch API key — "demo" works for low-volume testing
# Get a real key at https://portal.1inch.dev
ONEINCH_API_KEY=demo

# Wallet address advertised in the ACP offering manifest (optional)
AGENT_WALLET_ADDRESS=0xYourWalletAddressHere

# Logging filter (uses tracing-subscriber env-filter syntax)
RUST_LOG=swap_agent=debug,tower_http=info
```

All variables are optional — the agent will start with defaults if `.env` is absent.

**Jupiter API** requires no key and has no rate limits for public use.
**1inch API** uses `Bearer` token auth. With `demo`, production pairs may return errors — get a free key at [portal.1inch.dev](https://portal.1inch.dev).

---

## Building & Running

### From Source

**Prerequisites:** Rust toolchain (`rustc`, `cargo`) — install from [rustup.rs](https://rustup.rs) if needed.

```bash
# Clone and enter the project
git clone https://github.com/surfsurfmasurf/swap_agent.git
cd swap_agent

# (Optional) create your .env
cp .env.example .env   # or create manually — see Configuration above

# Development build + run (slower binary, faster compile)
cargo run

# Production build (optimized binary)
cargo build --release
./target/release/swap_agent
```

The server starts on `http://0.0.0.0:8080` by default.

To change the log level without editing `.env`:

```bash
RUST_LOG=debug cargo run
```

### As a systemd Service

The included systemd unit file runs the **release** binary and appends logs to `agent.log`:

```ini
[Unit]
Description=Swap Optimization Agent (Rust)
After=network.target

[Service]
Type=simple
User=<your-user>
WorkingDirectory=/path/to/swap_agent
EnvironmentFile=/path/to/swap_agent/.env
ExecStart=/path/to/swap_agent/target/release/swap_agent
Restart=always
RestartSec=5
StandardOutput=append:/path/to/swap_agent/agent.log
StandardError=append:/path/to/swap_agent/agent.log

[Install]
WantedBy=multi-user.target
```

```bash
# Build the release binary first
cargo build --release

# Install and enable
sudo cp swap_agent.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now swap_agent

# Check status
sudo systemctl status swap_agent

# Follow logs
tail -f agent.log
```

---

## API Reference

All endpoints return JSON. All request bodies are JSON (`Content-Type: application/json`).
Base URL: `http://localhost:8080`

---

### GET /health

Returns agent identity and status.

```bash
curl http://localhost:8080/health
```

```json
{
  "agent": "swap-optimization-agent",
  "status": "ok",
  "version": "0.1.0"
}
```

---

### GET /api/v1/price/solana

Fetch a live swap quote from Jupiter on Solana.

**Query parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `token_in` | string | yes | Input token symbol (e.g. `SOL`) |
| `token_out` | string | yes | Output token symbol (e.g. `USDC`) |
| `amount` | float | no | Amount of `token_in` (default: `1.0`) |

```bash
curl "http://localhost:8080/api/v1/price/solana?token_in=SOL&token_out=USDC&amount=1"
```

```json
{
  "chain": "Solana",
  "token_in": "SOL",
  "token_out": "USDC",
  "amount_in": 1.0,
  "amount_out": 148.32,
  "price_impact": 0.001,
  "fee": 0.25,
  "route": ["Orca", "Raydium"],
  "timestamp_ms": 1708612345678
}
```

---

### GET /api/v1/price/base

Fetch a live swap quote from 1inch on Base (chain ID 8453).

**Query parameters:** same as `/api/v1/price/solana`.

```bash
curl "http://localhost:8080/api/v1/price/base?token_in=ETH&token_out=USDC&amount=0.1"
```

```json
{
  "chain": "Base",
  "token_in": "ETH",
  "token_out": "USDC",
  "amount_in": 0.1,
  "amount_out": 329.41,
  "price_impact": 0.0,
  "fee": 0.3,
  "route": ["1inch Fusion"],
  "timestamp_ms": 1708612345999
}
```

---

### GET /api/v1/price/compare

Query both chains **simultaneously** and return a side-by-side comparison with the best chain highlighted.

Note: `token_in` and `token_out` must be valid on both chains. Cross-chain pairs that only exist on one chain will return `null` for the unavailable chain — the other chain's quote is still returned.

**Query parameters:** same as above.

```bash
curl "http://localhost:8080/api/v1/price/compare?token_in=USDC&token_out=USDT&amount=100"
```

```json
{
  "token_in": "USDC",
  "token_out": "USDT",
  "amount_in": 100.0,
  "solana_quote": {
    "chain": "Solana",
    "amount_out": 99.94,
    "fee": 0.25,
    "route": ["Orca"],
    ...
  },
  "base_quote": {
    "chain": "Base",
    "amount_out": 99.91,
    "fee": 0.3,
    "route": ["1inch Fusion"],
    ...
  },
  "best_chain": "Solana",
  "best_output": 99.94,
  "savings_pct": 0.03
}
```

---

### POST /api/v1/swap/quote

Returns the best quote across all chains for a given swap request, along with alternative quotes and a human-readable recommendation reason.

**Request body:**

| Field | Type | Required | Description |
|---|---|---|---|
| `token_in` | string | yes | Input token symbol |
| `token_out` | string | yes | Output token symbol |
| `amount_in` | float | yes | Amount of `token_in` to swap |
| `max_slippage_pct` | float | no | Max acceptable slippage % (default: 0.5) |
| `preferred_chain` | string | no | `"Solana"`, `"Base"`, or `"auto"` |
| `wallet_address` | string | no | Wallet address (for future live execution) |

```bash
curl -X POST http://localhost:8080/api/v1/swap/quote \
  -H "Content-Type: application/json" \
  -d '{
    "token_in": "SOL",
    "token_out": "USDC",
    "amount_in": 2.5,
    "max_slippage_pct": 0.5
  }'
```

```json
{
  "recommended": {
    "chain": "Solana",
    "token_in": "SOL",
    "token_out": "USDC",
    "amount_in": 2.5,
    "amount_out": 370.80,
    "price_impact": 0.002,
    "fee": 0.25,
    "route": ["Orca", "Raydium"],
    "timestamp_ms": 1708612346000
  },
  "alternatives": [
    {
      "chain": "Base",
      ...
    }
  ],
  "reason": "Solana is 0.12% better output"
}
```

---

### POST /api/v1/swap/execute

Submits a swap execution request. **Currently returns a simulation response** — live execution requires wallet integration (private key signing).

**Request body:** same as `/api/v1/swap/quote`.

```bash
curl -X POST http://localhost:8080/api/v1/swap/execute \
  -H "Content-Type: application/json" \
  -d '{
    "token_in": "SOL",
    "token_out": "USDC",
    "amount_in": 1.0
  }'
```

```json
{
  "status": "simulated",
  "message": "Live execution requires wallet integration",
  "request": { ... }
}
```

---

### GET /acp/offering

Returns the ACP (Agent Commerce Protocol) service manifest. This describes what services the agent offers, their input schemas, and payment details. AI agents use this endpoint to discover what the agent can do before invoking it.

```bash
curl http://localhost:8080/acp/offering
```

```json
{
  "agent_id": "swap-optimization-agent-v1",
  "agent_name": "Swap Optimization Agent",
  "description": "Real-time cross-chain swap optimizer: Solana (Jupiter) vs Base (1inch)",
  "version": "0.1.0",
  "services": [
    {
      "id": "price.compare",
      "name": "Multi-chain Price Comparison",
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
      "price_virtual": 0.5,
      "input_schema": {
        "token_in": "string",
        "token_out": "string",
        "amount_in": "number",
        "max_slippage_pct": "number (optional)",
        "preferred_chain": "string (optional)"
      }
    }
  ],
  "payment": {
    "token": "VIRTUAL",
    "wallet_address": "0xYourWalletAddress",
    "accepted_chains": ["Base", "Solana"]
  }
}
```

---

### POST /acp/invoke

Invoke an ACP service. The agent executes the service synchronously, stores the result in the in-memory job store, and returns the result immediately.

**Request body:**

| Field | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | `"price.compare"` or `"swap.best_quote"` |
| `params` | object | yes | Service-specific parameters (see offering schema) |
| `payment_tx` | string | no | Optional payment transaction hash |
| `callback_url` | string | no | Optional webhook URL for async notification |

**Example — price comparison:**

```bash
curl -X POST http://localhost:8080/acp/invoke \
  -H "Content-Type: application/json" \
  -d '{
    "service_id": "price.compare",
    "params": {
      "token_in": "SOL",
      "token_out": "USDC",
      "amount": 1.0
    }
  }'
```

**Example — best swap quote:**

```bash
curl -X POST http://localhost:8080/acp/invoke \
  -H "Content-Type: application/json" \
  -d '{
    "service_id": "swap.best_quote",
    "params": {
      "token_in": "SOL",
      "token_out": "USDC",
      "amount_in": 5.0,
      "max_slippage_pct": 1.0
    }
  }'
```

```json
{
  "job_id": "3f4a1b2c-...",
  "status": "done",
  "result": { ... }
}
```

---

### GET /acp/status/:job_id

Retrieve the stored result of a previously invoked ACP job by its UUID.

```bash
curl http://localhost:8080/acp/status/3f4a1b2c-...
```

```json
{
  "job_id": "3f4a1b2c-...",
  "status": "done",
  "result": { ... },
  "error": null,
  "created_at": 1708612346000,
  "updated_at": 1708612346120
}
```

Job statuses: `running` → `done` | `failed`.

---

## Testing

Run all endpoints with a single script. Requires `curl` and a running server on port 8080.

```bash
BASE="http://localhost:8080"

echo "=== Health ==="
curl -s $BASE/health | python3 -m json.tool

echo -e "\n=== Solana price: 1 SOL → USDC ==="
curl -s "$BASE/api/v1/price/solana?token_in=SOL&token_out=USDC&amount=1" | python3 -m json.tool

echo -e "\n=== Base price: 0.1 ETH → USDC ==="
curl -s "$BASE/api/v1/price/base?token_in=ETH&token_out=USDC&amount=0.1" | python3 -m json.tool

echo -e "\n=== Cross-chain compare: 100 USDC → USDT ==="
curl -s "$BASE/api/v1/price/compare?token_in=USDC&token_out=USDT&amount=100" | python3 -m json.tool

echo -e "\n=== Best swap quote ==="
curl -s -X POST $BASE/api/v1/swap/quote \
  -H "Content-Type: application/json" \
  -d '{"token_in":"SOL","token_out":"USDC","amount_in":2.5}' | python3 -m json.tool

echo -e "\n=== Swap execute (simulated) ==="
curl -s -X POST $BASE/api/v1/swap/execute \
  -H "Content-Type: application/json" \
  -d '{"token_in":"SOL","token_out":"USDC","amount_in":1.0}' | python3 -m json.tool

echo -e "\n=== ACP offering manifest ==="
curl -s $BASE/acp/offering | python3 -m json.tool

echo -e "\n=== ACP invoke: price.compare ==="
JOB=$(curl -s -X POST $BASE/acp/invoke \
  -H "Content-Type: application/json" \
  -d '{"service_id":"price.compare","params":{"token_in":"SOL","token_out":"USDC","amount":1.0}}')
echo $JOB | python3 -m json.tool
JOB_ID=$(echo $JOB | python3 -c "import sys,json; print(json.load(sys.stdin)['job_id'])")

echo -e "\n=== ACP job status ==="
curl -s "$BASE/acp/status/$JOB_ID" | python3 -m json.tool
```

**Expected results:**

| Endpoint | Expected HTTP | Notes |
|---|---|---|
| `GET /health` | 200 | Always succeeds |
| `GET /api/v1/price/solana` | 200 | Requires Jupiter API reachable |
| `GET /api/v1/price/base` | 200 | Requires valid `ONEINCH_API_KEY` |
| `GET /api/v1/price/compare` | 200 | Returns partial if one chain fails |
| `POST /api/v1/swap/quote` | 200 | Same deps as compare |
| `POST /api/v1/swap/execute` | 200 | Always returns simulated |
| `GET /acp/offering` | 200 | Always succeeds |
| `POST /acp/invoke` | 200 | Executes requested service |
| `GET /acp/status/:id` | 200 | Returns stored job or error |

**Troubleshooting:**

- `502 Bad Gateway` on price endpoints → the upstream DEX API is unreachable or returned an unexpected response. Check `RUST_LOG=debug` output in `agent.log`.
- `1inch` returning errors with `demo` key → sign up for a free key at [portal.1inch.dev](https://portal.1inch.dev) and set `ONEINCH_API_KEY` in `.env`.
- Port already in use → check `ss -tlnp | grep 8080` and kill the conflicting process or change `BIND_ADDR`.
