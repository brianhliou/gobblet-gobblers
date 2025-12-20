# Deployment Architecture

## Overview

The production deployment uses a WASM-based frontend with an embedded tablebase for position lookups.

```
┌─────────────────────┐     ┌──────────────────────────────────┐
│  Browser            │────▶│  Vercel                          │
│  - React UI         │     │  - Static files (dist/)          │
│  - WASM game logic  │     │  - Serverless Function           │
└─────────────────────┘     │    - tablebase.bin (170MB)       │
      ~33KB WASM            │    - Binary search lookups       │
                            └──────────────────────────────────┘
                                    <1ms lookup latency
```

## Components

### Frontend (Vercel Static)
- **Location:** `v2/frontend-wasm/`
- **Build:** `npm run build` → `dist/`
- **Size:** ~255KB total (32KB WASM, 67KB JS gzipped)
- **Game logic:** Runs entirely in browser via `gobblet-core` WASM package

### Tablebase API (Vercel Serverless Function)
- **Endpoint:** `POST /api/lookup/batch` with `{positions: string[]}`
- **Runtime:** Node.js (for file system access)
- **Data:** `tablebase.bin` (170MB, 19.8M positions)
- **Format:** Sorted array of (canonical: u64, outcome: i8) pairs
- **Lookup:** Binary search, O(log n) = ~24 comparisons per position

### Tablebase Binary Format
```
┌────────────────────────────────────────┐
│ Entry 0: canonical (8 bytes LE) + outcome (1 byte) │
│ Entry 1: canonical (8 bytes LE) + outcome (1 byte) │
│ ...                                                 │
│ Entry 19,836,039: ...                               │
└────────────────────────────────────────┘
Total: 178,524,360 bytes (170MB)
Sorted by canonical for binary search
```

## Performance

| Metric | Value |
|--------|-------|
| Cold start | ~300-500ms (read 170MB from disk) |
| Warm lookup (batch of 27) | <1ms |
| Memory usage | ~170MB per function instance |

## Local Development

### Without tablebase (game only)
```bash
cd v2/frontend-wasm
npm run dev
# Game works, evaluations show as unknown
```

### With tablebase (requires local Rust API)
```bash
# Terminal 1: Run local tablebase API
cd v2/gobblet-api
cargo run --release
# Serves on localhost:8000

# Terminal 2: Run frontend
cd v2/frontend-wasm
# Set VITE_API_URL=http://localhost:8000 in .env.local
npm run dev
```

## Deployment Steps

### 1. Generate tablebase binary (if needed)
```bash
cd v2/gobblet-solver
sqlite3 data/tablebase.db "SELECT canonical, outcome FROM positions ORDER BY canonical;" | \
python3 -c "
import sys, struct
data = bytearray()
for line in sys.stdin:
    c, o = line.strip().split('|')
    data.extend(struct.pack('<qb', int(c), int(o)))
with open('data/tablebase.bin', 'wb') as f:
    f.write(data)
"
cp data/tablebase.bin ../frontend-wasm/api/
```

### 2. Deploy to Vercel
```bash
cd v2/frontend-wasm
git add -A
git commit -m "Deploy with embedded tablebase"
git push origin main
# Vercel auto-deploys from git
```

## Cost Estimate

| Service | Free Tier | Our Usage | Cost |
|---------|-----------|-----------|------|
| Vercel (hosting) | 100GB bandwidth | Minimal | $0 |
| Vercel (Serverless) | 100GB-hrs | Minimal | $0 |

**Total: $0/month** for reasonable traffic.

## Architecture Decisions

### Why WASM instead of backend game logic?
- No server to maintain for game state
- Game works offline
- Lower latency (no round-trip for moves)
- Rust code already exists and is tested

### Why embedded binary instead of external database?
- **Sub-millisecond lookups** vs 500ms+ with external DB
- No network latency to external service
- Simpler architecture (no database to manage)
- Vercel replicates to all regions automatically
- 170MB fits within Vercel's deployment limits

### Why Serverless instead of Edge Functions?
- Edge Functions can't read files from disk
- Serverless Functions have file system access
- Cold start (~300ms) is acceptable for tablebase loading

## Regenerating the Tablebase

If the solver produces a new tablebase:

```bash
# 1. Export from SQLite to binary
cd v2/gobblet-solver
sqlite3 data/tablebase.db "SELECT canonical, outcome FROM positions ORDER BY canonical;" | \
python3 -c "
import sys, struct
data = bytearray()
for line in sys.stdin:
    c, o = line.strip().split('|')
    data.extend(struct.pack('<qb', int(c), int(o)))
with open('data/tablebase.bin', 'wb') as f:
    f.write(data)
print(f'Wrote {len(data)} bytes')
"

# 2. Copy to frontend
cp data/tablebase.bin ../frontend-wasm/api/

# 3. Deploy
cd ../frontend-wasm
git add api/tablebase.bin
git commit -m "Update tablebase"
git push
```
