# Frontend WASM

Production frontend for Gobblet Gobblers. Deployed to Vercel.

## Architecture

```
Browser                         Vercel
┌─────────────────────┐        ┌──────────────────────────┐
│ React UI            │───────▶│ Static files (dist/)     │
│ WASM game logic     │        │ Serverless function      │
│ (~32KB gobblet-core)│        │ └─ tablebase.bin (170MB) │
└─────────────────────┘        └──────────────────────────┘
```

- Game logic runs entirely in browser via WebAssembly
- Only API call is for tablebase position evaluations

## Development

```bash
npm install
npm run dev      # Frontend only (evaluations unavailable)
vercel dev       # Full stack with tablebase lookups
```

## Deployment

Automatic via Vercel on push to main. See [docs/deployment.md](../../docs/deployment.md).

## Key Files

- `src/App.tsx` - Main game component
- `api/lookup/batch.ts` - Serverless function for tablebase lookups
- `api/tablebase.bin` - Binary tablebase (Git LFS)
- `wasm-pkg/` - Compiled WASM from gobblet-core
