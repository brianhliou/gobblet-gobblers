# V2 - Rust Implementation

The current production implementation using Rust and WebAssembly.

## Components

### gobblet-core
Core game logic library written in Rust. Compiles to both native and WebAssembly.

- Board state representation and encoding
- Move generation with reveal rule handling
- Win/draw detection
- Canonical position encoding (symmetry reduction)

```bash
# Build WASM package
cd gobblet-core
wasm-pack build --target web
```

### gobblet-solver
Minimax solver with alpha-beta pruning. Generates the complete tablebase.

- Retrograde analysis from terminal positions
- Transposition table with Zobrist hashing
- Checkpoint/resume for long solves
- Exports to SQLite and binary formats

```bash
# Run solver (takes ~10 minutes)
cd gobblet-solver
cargo run --release

# Export to binary for deployment
cargo run --release --bin export_sqlite
```

### frontend-wasm
Production React frontend deployed to Vercel.

- Game UI with WASM-powered game logic
- Tablebase lookups via serverless function
- Move evaluation display (win/draw/loss coloring)

```bash
# Development
cd frontend-wasm
npm install
npm run dev        # Frontend only
vercel dev         # With tablebase API
```

### gobblet-api (deprecated)
Original REST API backend. Superseded by WASM architecture where game logic runs directly in the browser.

## Architecture

```
Browser                              Vercel
┌──────────────────────────┐        ┌─────────────────────────┐
│ React UI                 │        │ Static files            │
│ gobblet-core (WASM)      │───────▶│ Serverless function     │
│ - Move generation        │        │ └─ tablebase.bin        │
│ - Win detection          │        │    (170MB, 19.8M pos)   │
│ - State encoding         │        └─────────────────────────┘
└──────────────────────────┘
     Game logic runs                 Only tablebase lookups
     entirely in browser             go to server
```

## Building Everything

```bash
# 1. Build WASM package
cd gobblet-core
wasm-pack build --target web
cp -r pkg ../frontend-wasm/wasm-pkg

# 2. Build frontend
cd ../frontend-wasm
npm install
npm run build

# 3. Deploy (auto via Vercel on git push)
```
