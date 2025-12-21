# Gobblet Gobblers

A solved implementation of the Gobblet Gobblers board game with optimal play visualization.

**[Play the live demo](https://gobblet-gobblers-tablebase.vercel.app/)**

![Gobblet Gobblers Tablebase](docs/screenshot.png)

## About

Gobblet Gobblers is a Tic-Tac-Toe variant where:
- Each player has 6 pieces in 3 sizes (2 small, 2 medium, 2 large)
- Larger pieces can "gobble" (cover) smaller pieces
- Get 3 of your pieces visible in a row to win
- Moving a piece reveals what's underneath - be careful!

## The Solution

**Player 1 wins with perfect play.**

The solver uses minimax with alpha-beta pruning to determine the game-theoretic outcome. The tablebase contains 19.8 million positions - every position reachable when both players play optimally, plus all of Player 2's responses to any Player 1 move.

Some positions appear as "unknown" (gray). These arise only from suboptimal P1 play and were pruned during solving since they don't affect the optimal outcome. The complete game tree including all suboptimal lines is much larger and was not computed.

## Features

- **Optimal play hints** - Move colors show evaluation (green=winning, red=losing, yellow=draw)
- **WebAssembly game logic** - Runs entirely in browser, no server needed for gameplay
- **Tablebase lookups** - Sub-millisecond position evaluation via binary search

## Project Structure

```
gobblet-gobblers/
├── v2/                      # Current implementation (Rust + WASM)
│   ├── gobblet-core/        # Game logic (Rust, compiles to WASM)
│   ├── gobblet-solver/      # Minimax solver with alpha-beta pruning
│   ├── gobblet-api/         # REST API backend (deprecated)
│   └── frontend-wasm/       # React frontend + Vercel deployment
├── v1/                      # Original Python implementation (deprecated)
├── frontend-api/            # API-based frontend (deprecated)
└── docs/                    # Technical documentation
```

## Local Development

```bash
cd v2/frontend-wasm
npm install
npm run dev
```

Opens at http://localhost:5173. Game works fully; position evaluations require `vercel dev` or will show as unknown.

## Technical Details

- **Game logic**: Rust compiled to WebAssembly (~32KB)
- **Tablebase**: 170MB binary file with 19.8M positions
- **Lookup**: Binary search, O(log n) = ~24 comparisons per position
- **Hosting**: Vercel (static frontend + serverless function for tablebase)

See [docs/deployment.md](docs/deployment.md) for architecture details.

## Building the Solver

```bash
cd v2/gobblet-solver
cargo run --release
# Generates tablebase.db (~265MB SQLite)
# Convert to binary for deployment (see docs/deployment.md)
```

## Game Rules

See [docs/rules.md](docs/rules.md) for complete game rules including the "reveal" rule and edge cases.

## License

MIT
