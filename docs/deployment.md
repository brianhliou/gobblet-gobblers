# Deployment Architecture

The live system is two independently deployed pieces: a static **explorer**
(Vercel) that runs all gameplay in the browser via WASM, and an always-on
**tablebase probe** (`gobblet-api` on Railway) that answers position lookups out
of RAM.

```
┌──────────────────────────────┐        ┌─────────────────────────────────────┐
│  Browser  (Vercel static)    │        │  Railway  (always-on container)     │
│  explorer/ — React UI        │  POST  │  gobblet-api (Rust / axum)          │
│  gobblet-core WASM (~32 KB)  │ ─────▶ │  loads gobblet.ctb (~531 MB) → RAM  │
│  all moves run client-side   │ /lookup│  MPH lookup → signed DTM            │
└──────────────────────────────┘ /batch └─────────────────────────────────────┘
        no round-trip for gameplay                 ~µs per position
```

Gameplay is fully client-side — the API is consulted only to color moves by their
game-theoretic value (win / draw / loss + distance-to-mate).

## Components

### Explorer (`explorer/`, Vercel static)
- **Build:** `npm run build` → `dist/`
- **Game logic:** `gobblet-core` compiled to WASM, imported as the `gobblet-core`
  package (`file:./wasm-pkg`). No server needed to play.
- **API base URL:** `VITE_API_URL` (points at the Railway service).
- **Vercel project setting:** *Root Directory* = `explorer`.

### Tablebase probe (`api/`, Railway)
- **Endpoint:** `POST /lookup/batch` (alias `/api/lookup/batch`), plus `GET /health`.
  - request:  `{ "positions": ["<canonical-u64-decimal>", ...] }`
  - response: `{ "evaluations": [1|0|-1|null, ...], "dtm": [n|null, ...] }`
  - `evaluations`: 1 = P1 win, −1 = P2 win, 0 = draw, null = not found.
  - `dtm`: plies to result (`|distance-to-mate|`); 0 for draw/terminal; null if not found.
- **Lookup:** terminals resolved by `check_winner`; everything else via the MPH
  → one signed-DTM byte. The board is rejected if it isn't its own canonical key.
- **Env:** `CTB_PATH` (default `gobblet.ctb`), `PORT` (default 8080),
  `CORS_ORIGIN` (comma-separated allow-list; `*` or unset = any).
- **Image:** the repo-root `Dockerfile` builds `gobblet-api` and `ADD`s the `.ctb`
  from a GitHub Release at build time (`--build-arg CTB_URL=...`).
- **Railway project setting:** *Root Directory* = repo root, *Dockerfile path* = `Dockerfile`.

### Tablebase (`gobblet.ctb`)
A minimal perfect hash over the canonical keys of every **non-terminal** position,
plus one signed-DTM byte per position in MPH-slot order. Terminals are not stored
(the winner is on the board). It is **git-ignored** and distributed as a GitHub
Release asset, never committed.

```
[ MAGIC "GOBCTB01" : 8B ][ n : u64 LE ][ mph_len : u64 LE ][ mph : bincode ][ values : n × i8 ]
```

`n` ≈ 370.9 M non-terminal positions → ~531 MB total (values + MPH).

## Local development

```bash
# Explorer (gameplay only; evaluations need an API to point at)
cd explorer
npm install
npm run dev                       # http://localhost:5173

# Full stack: run the probe locally and point the explorer at it
cd ../api
CTB_PATH=/path/to/gobblet.ctb cargo run --release    # listens on :8080
# then start the explorer with VITE_API_URL=http://localhost:8080
```

## Regenerating the tablebase

The solver is the source of truth; the `.ctb` is a build artifact.

```bash
cd solver
cargo run --release --bin retro -- --save-ctb gobblet.ctb   # ~30 min, peak ~12 GB RAM on 14 cores
cargo run --release --bin retro -- --verify   gobblet.ctb   # re-check before shipping
```

Then upload `gobblet.ctb` to a GitHub Release and redeploy the Railway service
with the new `CTB_URL` build arg (or release tag).

## Deploy

- **Explorer:** Vercel auto-deploys `explorer/` on push to `main`.
- **API:** Railway rebuilds the root `Dockerfile`; trigger a redeploy when the
  `.ctb` changes (new release tag → new `CTB_URL`).
