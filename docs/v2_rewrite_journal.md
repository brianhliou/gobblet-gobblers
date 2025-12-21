# V2 Rewrite Journal

Notes from completing Milestones 1.1-1.10 (game logic + frontend integration).

## Overview

The rewrite was mostly straightforward - translating Python game logic to Rust with the same 64-bit board encoding. The Rust implementation closely mirrors V1's bit layout, making parity testing possible.

## Key Notes

### Bit-Level Optimizations

After the basic implementation, we added several optimizations for solver performance:

- **PackedMove (u8)**: Encodes any move in 1 byte vs 4+ for enum
- **PackedUndo (u16)**: Encodes undo info in 2 bytes vs struct with Options
- **MoveList**: Stack-allocated fixed array (64 moves max) vs heap Vec
- **Bitboard win detection**: Precomputed win masks, AND+CMP instead of loops
- **top_piece_fast**: Inlined bit extraction vs general top_piece method

These are optional APIs - the readable versions remain for clarity.

### Parity Test Churn

Initial parity testing showed 4 "failures" that weren't V2 bugs:

**Problem:** V1's `place_piece()` updates the board but not reserves. Edge case tests used this directly, creating impossible states:
- States where `board_pieces + reserves > 6` per player
- Invalid gobbles (e.g., Small placed on top of Medium)

**Solution:** Added `place_from_reserve()` helper that updates both board AND reserves. Fixed edge cases to use only valid gobbles.

**Lesson:** V2's design of deriving reserves from the board is more robust - it makes invalid states unrepresentable.

### Minor Issues

- **Cargo.toml edition**: Initially set to "2024" (doesn't exist), fixed to "2021"

### Milestone 1.10: Frontend Integration

Created `gobblet-api` Rust crate with Axum, implementing all V1-compatible endpoints:

**Architecture:**
- `gobblet-core`: Pure game logic, no serde/JSON
- `gobblet-api`: HTTP layer with JSON conversion

**Key decisions:**
- Added `encoding` field to `/game` response (raw u64 for debugging)
- State export uses decimal u64 instead of base64 (simpler, more transparent)
- Notation parsing lives in API layer (not core) since it's presentation concern

**All V1 endpoints preserved:**
- GET /game, /moves, /health
- POST /move, /reset
- GET /history, POST /undo, /redo, /goto/{index}
- GET /export, POST /import (move notation)
- GET /state/export, POST /state/import (u64 encoding)

Frontend unchanged - just connects to new Rust backend.

**Bug fix - Combined export format:**

When a state is imported (non-initial position) and then moves are played, exporting those moves alone would fail on reimport since they'd replay from initial position. Added combined format:

- `FROM:<encoding> <moves>` - captures starting position + moves
- Normal exports from initial position have no prefix
- Import detects `FROM:` prefix and loads that state first

Example: `FROM:536870928 M(2,2) L(0,1)` means "start from encoding 536870928, then play M(2,2) L(0,1)"

## Final State

- **109 tests passing** (106 unit + 3 parity integration)
- **50,009 positions verified** for V1/V2 parity
- **All API endpoints working** with frontend
- Phase 1 (game logic rewrite) complete

---

## Phase 1 Complete - 2024-12-19

Phase 1 (Milestones 1.1-1.10) is fully complete:
- gobblet-core: Rust game logic matching V1 behavior exactly
- gobblet-api: Axum server with all V1-compatible endpoints
- Frontend: Works unchanged with new Rust backend
- UI polish: hover highlighting, panel sizing, combined export format

**Next phase:** Solver implementation. See `v2_solver_planning.md`.
