# Gobblet Gobblers

## Project Overview

A complete, game-theoretic solution of Gobblet Gobblers (the 3×3 children's game),
with a browser explorer that colors every legal move by its win/draw/loss value and
distance-to-mate.

- **Live demo:** https://gobblet.brianhliou.com
- **Write-up:** https://brianhliou.com/posts/gobblet-gobblers/

## Project Status: COMPLETE

The first player wins with perfect play (13 plies from the opening). The game was solved
by **retrograde analysis**, not forward minimax — see "The solver" below for why that
distinction matters.

- 531.5M reachable positions; 370.9M non-terminal (185.4M first-player wins, 185.3M
  second-player wins, 208,563 draws ≈ 0.04%).
- Solve: ~30 min, peak ~12 GB RAM, 14 cores. State packs into a `u64`.

## Game Rules

See [docs/rules.md](docs/rules.md) for complete rules including the reveal rule and edge cases.

## Architecture

```
gobblet-gobblers/
├── core/        # game logic: bit-packed board, moves, win detection (compiles to WASM)
├── solver/      # retrograde solver (src/bin/retro.rs) + analysis bins; cuts the .ctb
├── api/         # tablebase probe (Rust/axum); loads the .ctb into RAM, serves lookups
├── explorer/    # React + WASM web explorer (deployed on Vercel)
├── legacy/
│   └── forward-solver/   # original forward minimax — GHI-buggy, preserved, NOT built
├── docs/        # rules, state encoding, GHI correction, game-tree analysis, deployment
├── assets/      # screenshot.png (README banner)
└── Dockerfile   # builds gobblet-api for Railway
```

The `v2/` wrapper and the old `v1/` Python implementation were removed in the cleanup;
their history is in git. The forward solver's last buildable state is the
`forward-solver-final` tag.

## How It Works

### Explorer (browser)
- React UI; game logic runs via WebAssembly (`gobblet-core` → WASM, ~32 KB). Gameplay is
  fully client-side, no round-trips.
- For move evaluations it POSTs canonical keys to the API (`VITE_API_URL`).

### Tablebase API (`gobblet-api` on Railway)
- `POST /lookup/batch` (alias `/api/lookup/batch`), `GET /health`.
  - request:  `{ "positions": ["<canonical-u64-decimal>", ...] }`
  - response: `{ "evaluations": [1|0|-1|null], "dtm": [n|null] }` (1 = P1 win, −1 = P2 win, 0 = draw)
- Loads `gobblet.ctb` into RAM; terminals resolved by `check_winner`, others via the MPH.
- Env: `CTB_PATH`, `PORT`, `CORS_ORIGIN`.

### Tablebase format (`gobblet.ctb`)
```
[ MAGIC "GOBCTB01" : 8B ][ n : u64 LE ][ mph_len : u64 LE ][ mph : bincode ][ values : n × i8 ]
```
Minimal perfect hash over non-terminal canonical keys + one signed-DTM byte each, ~531 MB.
Git-ignored; distributed as a GitHub Release asset, `ADD`ed into the image at build time.

## The solver (`solver/src/bin/retro.rs`)

Two-phase retrograde analysis, correct for a game with cycles:
1. BFS-enumerate every non-terminal canonical position (game-over boards are leaves).
2. Backward-induction fixpoint (parallel Jacobi) to signed distance-to-mate. Draws are the
   cycle-bound residue the fixpoint never proves won or lost.

Why not forward minimax: the threefold-repetition rule makes a position's value depend on
its history, so a transposition table keyed on the board alone is unsound (the **Graph
History Interaction** bug). The original forward solver lives in `legacy/forward-solver/`.

```bash
cd solver
cargo run --release --bin retro -- --selftest
cargo run --release --bin retro -- --save-ctb gobblet.ctb   # full solve
cargo run --release --bin retro -- --verify   gobblet.ctb
# analysis modes: --findings, --no-reveal, --no-stack (counterfactuals), --inspect-tb
```

Other solver bins: `render`, `count_tree`, `stats` (read-only analysis over the game graph).

## Conventions

- Rust: game logic in `core/`, solver in `solver/`, probe in `api/`. Crate package names
  keep the `gobblet-` prefix (`gobblet-core`, `gobblet-solver`, `gobblet-api`); only the
  directories are short. Path deps reference `../core`.
- TypeScript: explorer in `explorer/`. Imports the WASM package as `gobblet-core` (`file:./wasm-pkg`).
- Tablebase: compact `.ctb` for production; never committed (GitHub Release).

## Deploy coupling (paths live in dashboards, not the repo)

- **Vercel** (explorer): *Root Directory* = `explorer`.
- **Railway** (api): *Root Directory* = repo root, *Dockerfile path* = `Dockerfile`.

If the directory layout changes, update those two dashboard settings or the deploys break.
