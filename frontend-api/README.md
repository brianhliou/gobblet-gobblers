# Frontend API (DEPRECATED)

This was the original React frontend that communicated with a REST API backend
(`v2/gobblet-api` or `v1/api`) for all game logic operations.

## Architecture (deprecated)

```
Browser <--REST API--> gobblet-api (Rust) --> SQLite tablebase
```

- All game state, moves, history managed by backend
- Frontend was a thin UI layer

## Replaced by WASM Architecture

See `v2/frontend-wasm/` for the current implementation:

```
Browser (WASM game logic) <--API--> Vercel serverless --> tablebase.bin
```

- Game logic runs in browser via WebAssembly (gobblet-core compiled to WASM)
- Only backend call is for tablebase lookups
- Much faster, no round-trips for game operations
