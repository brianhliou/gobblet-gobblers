# Gobblet Gobblers

## Project Overview

A solved implementation of the Gobblet Gobblers board game with optimal play visualization.

**Live demo:** https://gobblet-gobblers-tablebase.vercel.app/

## Project Status: COMPLETE

- Full game implementation with web UI
- Solver using minimax with alpha-beta pruning (19.8 million positions)
- Optimal play visualization (move colors show win/draw/loss)

Player 1 wins with perfect play. The tablebase contains positions reachable via optimal play, plus all P2 responses. Positions from suboptimal P1 play were pruned and appear as "unknown".

## Game Rules

See [docs/rules.md](docs/rules.md) for complete rules including the reveal rule and edge cases.

## Architecture

```
gobblet-gobblers/
├── v2/                          # Current implementation (Rust + WASM)
│   ├── gobblet-core/            # Game logic library
│   │   ├── src/lib.rs           # Board, Move, Player, win detection
│   │   └── pkg/                 # Compiled WASM package
│   ├── gobblet-solver/          # Minimax solver
│   │   └── src/main.rs          # Alpha-beta with transposition tables
│   ├── gobblet-api/             # REST API backend (deprecated)
│   └── frontend-wasm/           # Production frontend
│       ├── src/App.tsx          # React game UI
│       ├── api/lookup/batch.ts  # Tablebase serverless function
│       └── api/tablebase.bin    # 170MB binary tablebase
├── v1/                          # Original Python (deprecated)
├── frontend-api/                # API-based frontend (deprecated)
└── docs/                        # Technical documentation
    ├── deployment.md            # Production architecture
    ├── rules.md                 # Game rules
    ├── state_encoding.md        # Canonical position encoding
    └── game_tree_analysis.md    # Solver analysis
```

## How It Works

### Frontend (Browser)
- React UI with piece selection and move highlighting
- Game logic runs via WebAssembly (gobblet-core compiled to WASM, ~32KB)
- No server round-trips for gameplay - fully client-side

### Tablebase API (Vercel Serverless)
- Single endpoint: `POST /api/lookup/batch`
- Looks up positions in 170MB binary tablebase
- Returns evaluation: 1 (P1 wins), 0 (draw), -1 (P2 wins)
- Binary search: O(log n) = ~24 comparisons per position

### Tablebase Format
```
Entry: [canonical: u64 LE] [outcome: i8]
       8 bytes              1 byte
Total: 19,836,040 entries × 9 bytes = 170MB
Sorted by canonical for binary search
```

## Running Locally

```bash
cd v2/frontend-wasm
npm install
npm run dev      # Game only (evaluations unavailable)
vercel dev       # Full stack with tablebase
```

## Key Implementation Details

### State Representation (gobblet-core)
- Board is 3x3 grid of stacks (up to 3 pieces per cell)
- Each cell encoded as 12 bits (4 bits per slot × 3 slots)
- Full board: 108 bits for cells + 24 bits for reserves = 132 bits
- Canonical form: min of all 8 symmetries (4 rotations × 2 reflections)

### Move Generation
- Reserve placements: any size to any empty or gobbleable square
- Board moves: move visible piece you own to valid destination
- Reveal rule: if lifting reveals opponent win, restrict to hail mary moves

### Solver (gobblet-solver)
- Minimax with alpha-beta pruning
- Transposition table with Zobrist hashing
- Move ordering: winning moves first, captures, center preference
- Result: 19.8M positions (optimal play + all P2 responses)

## Conventions

- Rust: Game logic in gobblet-core, solver in gobblet-solver
- TypeScript: Frontend in v2/frontend-wasm
- Tablebase: Binary format for production, SQLite for solver development
- Git LFS: Used for tablebase.bin (170MB)
