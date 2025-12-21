# V1 - Python Implementation (DEPRECATED)

Original Python implementation of Gobblet Gobblers, now replaced by Rust (v2).

## Components

- `gobblet/` - Core game logic (state, moves, win detection)
- `api/` - FastAPI REST backend
- `solver/` - Minimax solver with transposition tables
- `tests/` - Unit tests

## Replaced by

- `v2/gobblet-core/` - Rust game logic (also compiles to WASM)
- `v2/gobblet-solver/` - Rust solver (much faster)
- `v2/frontend-wasm/` - React frontend with WASM game logic

The Rust implementation is significantly faster and enables running game
logic directly in the browser via WebAssembly.
