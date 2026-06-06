# explorer

Interactive web explorer over the solved Gobblet Gobblers tablebase. React UI with the
game logic running in-browser via WebAssembly (`gobblet-core`); move evaluations come from
the tablebase probe (`gobblet-api`) over HTTP.

```
Browser (Vercel static)                    Railway
┌──────────────────────────┐   POST       ┌─────────────────────────────┐
│ React UI                 │ ──────────▶  │ gobblet-api (Rust / axum)   │
│ WASM game logic (~32 KB) │ /lookup/batch│ gobblet.ctb in RAM → DTM    │
└──────────────────────────┘              └─────────────────────────────┘
```

Gameplay is fully client-side; the API is consulted only to color moves by value and
distance-to-mate.

## Development

```bash
npm install
npm run dev                         # http://localhost:5173
# set VITE_API_URL to a running gobblet-api for evaluations (see ../docs/deployment.md)
```

## Deployment

Vercel, auto-deploys on push to `main`. *Root Directory* = `explorer`. Full architecture in
[../docs/deployment.md](../docs/deployment.md).

## Key files

- `src/App.tsx` — main game component
- `src/api.ts` — tablebase lookup client (`VITE_API_URL`)
- `wasm-pkg/` — compiled WASM from `core/` (imported as the `gobblet-core` package)
