# CEX — Centralized Exchange Engine (Rust / Actix-Web)

A minimal centralized exchange backend written in Rust. It exposes authenticated HTTP endpoints for user onboarding, fiat on-ramping, asset deposits, and balance queries, with balance state serialized through dedicated worker threads via `mpsc` channels — an actor-like isolation pattern to avoid shared-mutable-state races.

> **Status:** Early prototype / learning project. Persistence is in-memory, passwords are stored in plaintext, and the orderbook matching engine is stubbed but not yet wired to the HTTP layer. Not production-ready.

---

## Table of Contents

- [Architecture](#architecture)
- [Tech Stack](#tech-stack)
- [Project Structure](#project-structure)
- [Getting Started](#getting-started)
- [API Reference](#api-reference)
- [Data Flow](#data-flow)
- [Configuration](#configuration)
- [Known Limitations & Roadmap](#known-limitations--roadmap)
- [Development](#development)

---

## Architecture

```
                ┌─────────────┐
  Client ──────►│  Actix-Web   │──────┐
                │  HttpServer  │      │
                │  :8080       │      ▼
                └─────────────┘  AppState
                                     │
                    ┌────────────────┼────────────────┐
                    │                │                │
              Mutex<Vec<User>>  mpsc::Sender     mpsc::Sender
                                 │ BalanceMessage  │ StockMessage
                                 ▼                ▼
                          ┌────────────┐   ┌──────────────┐
                          │ USD Worker │   │ Stock Worker │
                          │  Thread    │   │   Thread     │
                          │ HashMap<   │   │ HashMap<Uuid,│
                          │  Uuid,u32> │   │  HashMap<    │
                          └────────────┘   │  String,u32>>│
                                           └──────────────┘
```

**Key design decisions:**

| Decision | Rationale |
|---|---|
| **Single-owner worker threads for balances** (`src/main.rs:39-75`) | Each balance domain (USD, stocks) is owned by exactly one thread that processes messages sequentially via `std::sync::mpsc`. This eliminates the need for locking on the hot path and prevents race conditions on balance mutations without requiring a DB transaction. |
| **`oneshot` channels for reads** | `GET /balance` sends a `GetBalance` / `GetBalances` message carrying a `futures::channel::oneshot::Sender` and `await`s the reply, bridging the sync worker thread back to the async Actix handler. |
| **`AuthUser` extractor** (`src/middleware/auth.rs:30-56`) | Implements `FromRequest` so any handler with an `AuthUser` parameter automatically validates the `Authorization: Bearer <JWT>` header. Keeps auth logic out of route handlers. |
| **In-memory storage** | `Mutex<Vec<User>>` and `HashMap` balances are intentionally simple for prototyping; designed to be swapped for a database + Redis without changing the message interface. |

---

## Tech Stack

| Crate | Version | Purpose |
|---|---|---|
| `actix-web` | 4.15 | HTTP server & routing |
| `jsonwebtoken` | 11.0 (`rust_crypto`) | JWT sign/verify (HS256) |
| `serde` / `serde_json` | 1.0 | Request/response serialization |
| `uuid` | 1.26 (`v4`, `serde`) | User & order IDs |
| `futures` | 0.3 | `oneshot` channel for worker replies |

- **Language:** Rust 2024 edition
- **Runtime:** Tokio (via `actix-web`)

---

## Project Structure

```
cex/
├── Cargo.toml
├── src/
│   ├── main.rs              # AppState, message enums, worker spawns, server bootstrap
│   ├── engine/              # (empty) placeholder for matching engine
│   ├── middleware/
│   │   ├── mod.rs
│   │   └── auth.rs          # JWT validation, AuthUser extractor, AuthError
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── user.rs          # signup, signin, balance, onramp, deposit handlers
│   │   └── order.rs         # Order / Orderbook structs (stub, not yet routed)
│   └── types/
│       ├── mod.rs
│       └── user.rs          # DTOs: SignupInputs, Claims, BalanceResponse, etc.
└── target/                  # build artifacts (gitignored)
```

### Module Overview

| Module | File | Responsibility |
|---|---|---|
| `main` | `src/main.rs:11-25` | Defines `BalanceMessage`, `StockMessage`, `AppState`; spawns USD & stock worker threads; binds `HttpServer` on `127.0.0.1:8080` |
| `middleware::auth` | `src/middleware/auth.rs` | `AuthUser(Uuid)` extractor, `JWT_SECRET = "secret"`, `AuthError` → `400 { message }` |
| `routes::user` | `src/routes/user.rs` | 5 endpoints: `POST /signup`, `POST /signin`, `GET /balance`, `POST /onramp`, `POST /deposit/{symbol}` |
| `routes::order` | `src/routes/order.rs` | `Order` & `Orderbook` definitions (`BTreeMap<u32, Order>` for asks/bids) — currently unused |
| `types::user` | `src/types/user.rs` | Serde DTOs and `User` / `Cliams` structs |

---

## Getting Started

### Prerequisites

- Rust stable toolchain (`rustup` ≥ 1.75, edition 2024)
- `cargo` on `PATH`

### Install & Run

```bash
# clone
git clone https://github.com/anushkkka19/exchange.git
cd exchange

# run in debug mode (listens on 127.0.0.1:8080)
cargo run

# or build an optimized binary
cargo build --release
./target/release/cex
```

Server starts with no extra configuration:

```
cargo run
# → HttpServer listening on 127.0.0.1:8080
```

### Quick Smoke Test

```bash
# 1. Sign up
curl -X POST http://127.0.0.1:8080/signup \
  -H "Content-Type: application/json" \
  -d '{"email":"alice@example.com","password":"s3cret"}'

# 2. Sign in → capture token
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/signin \
  -H "Content-Type: application/json" \
  -d '{"email":"alice@example.com","password":"s3cret"}' | jq -r .token)

# 3. On-ramp USD
curl -X POST http://127.0.0.1:8080/onramp \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"amount":5000}'

# 4. Deposit an asset
curl -X POST http://127.0.0.1:8080/deposit/AAPL \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"qty":10}'

# 5. Check balances
curl http://127.0.0.1:8080/balance \
  -H "Authorization: Bearer $TOKEN" | jq
# → {"usd_balance":5000,"stock_balance":{"AAPL":10}}
```

---

## API Reference

Base URL: `http://127.0.0.1:8080`

All authenticated endpoints require:

```
Authorization: Bearer <JWT>
```

JWT is HS256, secret `secret`, expiry 24h (`src/routes/user.rs:59-69`, `src/middleware/auth.rs:9`).

### `POST /signup`

Create a new user.

- **Auth:** none
- **Body:**

```json
{
  "email": "alice@example.com",
  "password": "s3cret"
}
```

- **Responses:**

| Status | Body |
|---|---|
| `200` | `{"message":"User created!"}` |
| `409` | `{"message":"User Already Exists"}` |

> Source: `src/routes/user.rs:22-45`

### `POST /signin`

Authenticate and receive a JWT.

- **Auth:** none
- **Body:** same as signup
- **Responses:**

| Status | Body |
|---|---|
| `200` | `{"message":"Ok","token":"<JWT>"}` |
| `401` | `{"message":"Invalid Creds","token":null}` |
| `404` | `{"message":"User not found","token":null}` |

> Source: `src/routes/user.rs:47-81`

### `GET /balance`

Return USD and per-symbol stock balances for the authenticated user.

- **Auth:** required
- **Response `200`:**

```json
{
  "usd_balance": 5000,
  "stock_balance": {
    "AAPL": 10,
    "TSLA": 5
  }
}
```

Uses two `oneshot` round-trips to the worker threads (`src/routes/user.rs:83-103`).

### `POST /onramp`

Credit USD balance (simulates fiat deposit).

- **Auth:** required
- **Body:**

```json
{ "amount": 1000 }
```

- **Response:** `200 OK` (empty body on success), `400` on channel error.

> Source: `src/routes/user.rs:105-126` — note the handler currently returns `200 OK` regardless of the inner `match` result (missing `return`).

### `POST /deposit/{symbol}`

Credit a stock/asset balance for the given ticker symbol.

- **Auth:** required
- **Path param:** `symbol` — e.g. `AAPL`, `BTC`
- **Body:**

```json
{ "qty": 10 }
```

- **Responses:**

| Status | Body |
|---|---|
| `200` | `{"message":"Deposit Successful"}` |
| `500` | `{"message":"depsit failed."}` |

> Source: `src/routes/user.rs:128-150`

### Error Responses (Auth)

When the `Authorization` header is missing or the JWT is invalid:

```json
{ "message": "Invalid or missing token" }
```

Status `400` (`src/middleware/auth.rs:20-26`).

---

## Data Flow

### Write Path (On-Ramp / Deposit)

```
Handler (async) ──mpsc::send──► Worker Thread (sync loop)
                                  ├─ HashMap lookup
                                  ├─ insert(updated_balance)
                                  └─ (no reply needed)
```

### Read Path (Balance)

```
Handler ──mpsc::send(GetBalance, oneshot::Sender)──► Worker Thread
   │                                                    ├─ lookup
   │                                                    └─ oneshot::send(balance)
   ◄──────────────── oneshot::recv ─────────────────────┘
Handler ──mpsc::send(GetBalances, oneshot::Sender)──► Stock Worker
   │                                                    └─ oneshot::send(cloned HashMap)
   ◄──────────────── oneshot::recv ─────────────────────┘
Handler ──► HttpResponse::Ok(BalanceResponse)
```

---

## Configuration

| Parameter | Current Value | Location | Notes |
|---|---|---|---|
| Bind address | `127.0.0.1:8080` | `src/main.rs:86` | Hardcoded; make configurable via env var for deployment |
| JWT secret | `"secret"` | `src/middleware/auth.rs:9` | Hardcoded; must be moved to `JWT_SECRET` env var |
| JWT expiry | 24 hours | `src/routes/user.rs:63` | `SystemTime + 86400s` |
| Password storage | plaintext | `src/routes/user.rs:32` | Comment in code notes it "should ideally be hashed" — use `argon2`/`bcrypt` |

---

## Known Limitations & Roadmap

**Current limitations:**

- No persistence — all state is lost on restart.
- Passwords stored in plaintext (`src/types/user.rs:23-27`).
- `Orderbook` in `src/routes/order.rs` is defined but never instantiated or exposed via routes; `src/engine/` is empty.
- `POST /onramp` handler does not propagate the `match` response correctly (`src/routes/user.rs:115-125` always returns `200`).
- `Cliams` typo in `src/types/user.rs:30` (should be `Claims`).
- No input validation (email format, negative amounts).
- No rate limiting, CORS, or request logging.
- Single-instance only — `mpsc` channels do not work across processes.

**Roadmap:**

- [ ] Hash passwords with `argon2` and add email validation
- [ ] Persist users & balances (Postgres / SQLite + migrations)
- [ ] Implement matching engine in `src/engine/` and wire `Orderbook` to `POST /order` / `DELETE /order` routes
- [ ] Fix `onramp` response handling and `Cliams` naming
- [ ] Externalize config (bind addr, JWT secret) to environment variables
- [ ] Add `GET /trades`, `GET /orderbook/{symbol}`, websocket price feed
- [ ] Integration tests with `actix-rt` and `cargo test`

---

## Development

```bash
# check without building
cargo check

# run with logs
RUST_LOG=debug cargo run

# format & lint
cargo fmt
cargo clippy
```

### Contributing

1. Fork the repo and create a feature branch.
2. Ensure `cargo fmt` and `cargo clippy` pass.
3. Open a PR with a clear description of the change.

---

## License

No license file is currently present. If you intend to open-source this, consider adding an `MIT` or `Apache-2.0` license.

---

<p align="center">
  Built with Rust + Actix-Web. PRs welcome.
</p>
