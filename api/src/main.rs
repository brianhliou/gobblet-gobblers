//! Gobblet Gobblers tablebase probe.
//!
//! Loads the compact minimal-perfect-hash tablebase (`gobblet.ctb`, built by
//! `retro --save-ctb`) into RAM and serves the batch lookup the frontend already
//! calls:
//!
//!   POST /lookup/batch   (also /api/lookup/batch)
//!     request:  { "positions": ["<canonical-u64-decimal>", ...] }
//!     response: { "evaluations": [1|0|-1|null, ...], "dtm": [n|null, ...] }
//!
//!   evaluations: 1 = P1 win, -1 = P2 win, 0 = draw, null = not found.
//!   dtm: plies to result (|distance-to-mate|); 0 for draw/terminal; null if not found.
//!
//! Env: CTB_PATH (default "gobblet.ctb"), PORT (default 8080).

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, StatusCode},
    routing::{get, post},
    Json, Router,
};
use boomphf::Mphf;
use gobblet_core::{Board, Player};
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::timeout::TimeoutLayer;

const MAGIC: &[u8; 8] = b"GOBCTB01";

/// Abuse guards. The viewer sends ~tens of positions per batch, so these only
/// ever fire on malformed or hostile requests; normal latency is unaffected.
const MAX_BATCH: usize = 1024; // reject batch-bomb requests
const MAX_BODY: usize = 256 * 1024; // 256 KB request body
const REQ_TIMEOUT: Duration = Duration::from_secs(10);

/// Compact tablebase: a minimal perfect hash over canonical keys + one signed
/// DTM byte per non-terminal position in MPH-slot order. Terminals are NOT stored
/// — they are resolved by `check_winner` before any MPH lookup.
struct Tb {
    mph: Mphf<u64>,
    values: Vec<u8>, // i8 DTM reinterpreted; index = mph slot
}

impl Tb {
    fn load(path: &str) -> std::io::Result<Tb> {
        let mut f = std::fs::File::open(path)?;
        let mut hdr = [0u8; 24];
        f.read_exact(&mut hdr)?;
        assert_eq!(&hdr[0..8], MAGIC, "not a gobblet.ctb file");
        let mph_len = u64::from_le_bytes(hdr[16..24].try_into().unwrap()) as usize;
        let mut mph_buf = vec![0u8; mph_len];
        f.read_exact(&mut mph_buf)?;
        let mph: Mphf<u64> = bincode::deserialize(&mph_buf).expect("deserialize mph");
        drop(mph_buf);
        let mut values = Vec::new();
        f.read_to_end(&mut values)?;
        Ok(Tb { mph, values })
    }

    /// (absolute outcome, dtm) for a canonical key, or None if not in the set.
    fn lookup(&self, key: u64) -> Option<(i8, i32)> {
        let b = Board::from_u64(key);
        // Reject anything that isn't a canonical key: the frontend always sends
        // canonical encodings, so this filters malformed/garbage input before the
        // MPH (which has no true membership test) can false-positive on it.
        if b.canonical() != key {
            return None;
        }
        // Terminal (game over): the winner is on the board; not in the MPH set.
        if let Some(w) = b.check_winner() {
            return Some((if w == Player::One { 1 } else { -1 }, 0));
        }
        let slot = self.mph.try_hash(&key)? as usize;
        let dtm = *self.values.get(slot)? as i8 as i32;
        let abs = match dtm.signum() {
            1 => {
                if b.current_player() == Player::One {
                    1
                } else {
                    -1
                }
            }
            -1 => {
                if b.current_player() == Player::One {
                    -1
                } else {
                    1
                }
            }
            _ => 0,
        };
        Some((abs, dtm.unsigned_abs() as i32))
    }
}

#[derive(Deserialize)]
struct BatchReq {
    positions: Vec<String>,
}

#[derive(Serialize)]
struct BatchResp {
    evaluations: Vec<Option<i8>>,
    dtm: Vec<Option<i32>>,
}

async fn lookup_batch(
    State(tb): State<Arc<Tb>>,
    Json(req): Json<BatchReq>,
) -> Result<Json<BatchResp>, (StatusCode, &'static str)> {
    if req.positions.len() > MAX_BATCH {
        return Err((StatusCode::BAD_REQUEST, "batch too large"));
    }
    let mut evaluations = Vec::with_capacity(req.positions.len());
    let mut dtm = Vec::with_capacity(req.positions.len());
    for p in &req.positions {
        match p.parse::<u64>().ok().and_then(|k| tb.lookup(k)) {
            Some((abs, d)) => {
                evaluations.push(Some(abs));
                dtm.push(Some(d));
            }
            None => {
                evaluations.push(None);
                dtm.push(None);
            }
        }
    }
    Ok(Json(BatchResp { evaluations, dtm }))
}

/// CORS from `CORS_ORIGIN` (comma-separated allow-list; "*" or unset = any).
/// Default any keeps the viewer working before we know its exact deploy origin;
/// set it at deploy to lock down browser callers.
fn cors_layer() -> CorsLayer {
    let base = CorsLayer::new().allow_methods(Any).allow_headers(Any);
    match std::env::var("CORS_ORIGIN") {
        Ok(s) if s != "*" && !s.trim().is_empty() => {
            let list: Vec<HeaderValue> =
                s.split(',').filter_map(|o| o.trim().parse().ok()).collect();
            base.allow_origin(AllowOrigin::list(list))
        }
        _ => base.allow_origin(Any),
    }
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let ctb = std::env::var("CTB_PATH").unwrap_or_else(|_| "gobblet.ctb".into());
    eprintln!("loading {ctb} ...");
    let tb = Arc::new(Tb::load(&ctb).expect("load ctb"));
    eprintln!("loaded {} value bytes", tb.values.len());

    let app = Router::new()
        .route("/health", get(health))
        .route("/lookup/batch", post(lookup_batch))
        .route("/api/lookup/batch", post(lookup_batch))
        .layer(cors_layer())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQ_TIMEOUT,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(tb);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    eprintln!("gobblet-api listening on {addr}");
    axum::serve(listener, app).await.expect("serve");
}
