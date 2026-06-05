# Tablebase Correction & Infra Plan (retrograde recompute)

> Status: ground truth **trusted**; architecture **hybrid**; eval depth **DTM (win-in-N)**.
> **A1 (DTM solver), A2 (gobblet.ctb = 531 MB), and Phase B CODE (probe + Dockerfile + abuse
> guards, tested locally) all DONE.** Rules-mismatch question **RESOLVED** (non-terminal-missing
> = 0). Fingerprint **skipped** (viewer always in-set; risk is resource-abuse, not wrong-answers).
> **DEPLOYED:** `gobblet.ctb` published as Release `tb-v1`; Railway service `gobblet-gobblers`
> (project `44b22ad1-9a81-4073-bd37-2a4b530f91b0`) built from `v2/Dockerfile` via `railway up` and
> LIVE at **https://gobblet-gobblers-production.up.railway.app** — `/health` ok, `/lookup/batch`
> returns correct evals + DTM (init → P1-win DTM 13), oversized batch → 400. **Vercel repointed:
> `gobblet.brianhliou.com` now has the Railway URL baked into its bundle — the public viewer
> serves the corrected tablebase (the 23,943 wrong entries are gone). FIX SHIPPED.** Remaining
> (cleanup): merge PR #11; tighten `CORS_ORIGIN` → `https://gobblet.brianhliou.com`; Cloudflare in
> front of gobblet + dobutsu; C (LFS cleanup, retire Vercel fn); D (writing).

## Why we revisited

The V1/V2 solvers used **forward DFS minimax + a transposition table + on-path cycle
detection returning DRAW**. That is the classic **Graph-History-Interaction (GHI)** bug:
path-dependent cycle-draw values get cached in a position-keyed table and reused from other
paths, corrupting results. See `docs/game_tree_analysis.md` — the bug was diagnosed there but
never correctly fixed (the project shipped the alpha-beta result + runtime repetition
handling).

We recomputed with **retrograde analysis** (method ported from the `dobutsu-shogi` project):
enumerate every canonical reachable position, then fill win/loss/draw by backward-induction
fixpoint. Draws are NOT detected by repetition — they are the cycle-bound positions the
fixpoint can never prove won or lost (the UNKNOWN residue at convergence). No path, no
history → GHI cannot occur. Solver: `v2/gobblet-solver/src/bin/retro.rs`.

## Ground-truth result

```
canonical reachable positions   531,557,711
P1-win  265,965,213    P2-win  265,383,935    draw  208,563
INITIAL POSITION:  Player 1 wins
```

- The **headline "P1 wins" is confirmed** — independent, GHI-free method agrees at the root.
  The old full-solve "DRAW" was the bug; alpha-beta's "P1 wins" was right because P1's winning
  strategy is "pure" (reaches terminals without needing cycles).
- The state-space size **531,557,711** is independently confirmed (matches the old solve's count).
- Draws are rare: 208,563 of 531.6M (0.04%); W/L split is near-perfectly balanced.

**Calibrated-DTM solve (Phase A1, terminals excluded — the artifact we serve):**

```
non-terminal positions   370,974,636
P1-win 185,455,752   P2-win 185,310,321   draw 208,563
max win-DTM 23   max loss-DTM 24
START POSITION:  Player 1 wins in 13 plies
```

- `draw 208,563` is **identical** to the W/L/D solve — strongest cross-check that the DTM
  rewrite (parallel Jacobi, terminal-exclusion) is correct.
- **max DTM 24** ⇒ values fit in **6 bits** (sign + 5 magnitude) → a very small compact TB.
- Game-over boards are excluded from the node set (the viewer detects them locally via
  check_winner); 370,974,636 = 531,557,711 − 160,583,075 terminals.
- Implementation notes: compact custom open-addressing `key→id` table (~8.6 GB at 0.74 load,
  fits a 13 GB free-RAM budget); **parallel Rayon fixpoint** with `AtomicI8` values
  (race-free; correct because values are monotone once set) — the fixpoint runs in ~2.5 min
  across 14 cores vs ~2.5 hr serial. Full run ≈ 30 min, peak ~11.9 GB.

## The bug's footprint in the published tablebase

`retro --verify ../frontend-wasm/api/tablebase.bin` (the shipped 19,836,040-entry pruned TB):

```
checked 19,795,329    mismatches 23,943
```

- **23,943 published positions (0.12%) are wrong** — every observed mismatch is
  `published = draw, correct = decisive (win/loss)`. That is the exact GHI signature: spurious
  draws from cached cycle-draws. The root is not among them, which is why the public headline
  survived while ~24K interior evaluations are corrupted.
- => The deployed viewer at gobblet.brianhliou.com shows **wrong evals for ~24K positions** and
  is built on the buggy TB. It needs to be replaced with the corrected solve.

## The 40,711 "unreachable" keys + the rules-mismatch question

> **RESOLVED (Phase A1).** With game-over boards excluded, the DTM run's verify reports
> `terminal-skipped 12,700,812, NON-TERMINAL-MISSING 0`. Every one of the 40,711 was a terminal
> position (the old TB stored terminals; we don't). There is **zero non-terminal reachability
> discrepancy** → our enumeration is complete and no rules drift affected anything reachable.
> The accounting is exact: 7,135,228 non-terminal checked + 12,700,812 terminal-skipped =
> 19,836,040 (the full file). The original investigation below stands as the reasoning.

`verify` found **40,711 published keys not in our reachable set** (19,836,040 − 19,795,329).
`retro --inspect-tb` on the published TB:

```
total 19,836,040    non-canonical 0    terminal 12,700,812
```

- **All published keys are canonical** → no canonicalization drift.
- Our BFS is complete by construction → the 40,711 are **not reachable under the current
  rules**, so the current viewer can never navigate to them.
- **Rules-mismatch caveat (noted, not blocking):** `tablebase.bin` was committed (`34dfb2d`,
  2025-12-20 23:53) ~14 h **before** the source's first commit (`65a3fd2`, 2025-12-21 13:42).
  `gobblet-core/src/lib.rs` and `gobblet-solver/src/movegen.rs` (which holds `check_reveal`)
  each have exactly one commit and zero edits since — so there is **no rules-change event in
  tracked history** — but git cannot prove the TB-generating code was byte-identical in that
  pre-commit window. It does not threaten the new TB: our solve and the live viewer use the
  same current `gobblet-core`, so they are mutually consistent regardless.
- **To fully close** (fold into the re-solve): (1) dump the 40,711 and confirm they are
  unreachable / post-terminal; (2) brute-check a sample of the 23,943 with the independent
  current-code oracle — if it says "decisive," the old "draw" is wrong under current rules,
  GHI-or-not.

## Infra plan — DECIDED: **hybrid**

Mirror dobutsu's serving (compact MPH tablebase + Rust probe + container on Railway), but keep
the React/Vite frontend on Vercel's CDN. The frontend's eval API base is already a single env
var (`src/api.ts`: `API_BASE = import.meta.env.VITE_API_URL ?? "/api"`), so repointing is a
one-liner.

- **Frontend:** stays on Vercel (static + WASM game logic).
- **Tablebase API:** moves to a Railway always-on container holding the compact TB in RAM.
- **Repoint:** set `VITE_API_URL` → Railway URL; add CORS for `brianhliou.com`.
- **Retire:** the embedded 170 MB `api/tablebase.bin` + the Vercel `api/lookup/batch.ts` function.

### Compact tablebase — BUILT (Phase A2)

`retro --save-ctb data/gobblet.ctb` produced **`gobblet.ctb` = 531 MB** (160 MB MPH + 371 MB of
1-byte signed DTM), MPH round-trip self-check 5001/5001. Format `GOBCTB01`:
`MAGIC(8) | n:u64 | mph_len:u64 | mph(bincode boomphf::Mphf<u64>) | n × i8 DTM` (in MPH-slot
order). **No fingerprint** — `NON-TERMINAL-MISSING = 0`, so every legitimate query is in-set; the
probe handles terminals via `check_winner` before any MPH lookup. Lookup: `try_hash(key)?` →
`values[slot] as i8` DTM (sign = outcome). max |DTM| = 24 ⇒ comfortably in one byte.

### Compact tablebase format (original plan)

Mirror dobutsu's `.ctb` (`solver/src/bin/compact.rs`): minimal perfect hash (`boomphf::Mphf`)
over canonical keys + packed values. **DECIDED: include distance-to-result** ("win in N", like
dobutsu's viewer) — so values are ~9-bit DTM, not 2-bit W/L/D.

- An MPH returns garbage for out-of-set keys → add a **key fingerprint** (~8–16 bits/position):
  on lookup, MPH→slot, check fingerprint; mismatch → "unknown". Catches the 40,711 (and any
  out-of-set query). Estimated size with DTM + fingerprint ≈ ~600 MB – 1 GB (vs 4.78 GB raw).
  Raw sorted + binary search (4.78 GB, mmap) remains the inherently-safe fallback.

## Public readiness (going public)

Secret audit (two independent passes) — **clean, safe to make public**:
- Manual: no real `.env` ever committed (only `.env.example`, placeholders); `.gitignore` covers
  `.env`, `.env.local`, `.env*.local`; zero hits for JWT / `authToken` / `libsql` / `sk-` /
  `ghp_` / AWS-key patterns in tracked files; the one Turso mention (`deployment_planning.md`) is
  architectural prose, not a credential.
- `gitleaks` 8.30.1 full-history scan (36 commits): **0 findings**.

One cleanup — not a secret issue: `v2/frontend-wasm/api/tablebase.bin` is a **178 MB Git-LFS
object**, and it's the *wrong* (buggy) data. Public repo + LFS hits bandwidth/storage limits
(GitHub free LFS ≈ 1 GB storage + 1 GB/mo bandwidth → ~5–6 clones exhausts it). Remove it during
the migration and serve the corrected `.ctb` from a **GitHub Release** (Releases don't count
against LFS), as in Phase C. Fully scrubbing the old blob from LFS *history* is optional and needs
a `git-filter-repo` rewrite.

The bug docs (`game_tree_analysis.md`, this file) are an asset for a public portfolio repo — a
"found and fixed a subtle GHI bug in my own solver" story — not a liability.

## Build steps

**Phase A — corrected compact tablebase** (solver work)
1. Add **calibrated DTM** to `retro`: detect winning moves parent-side (don't materialize the
   already-won board as a node) so depth is exact, not off-by-one. (Current solver is W/L/D only.)
2. Add a **compact MPH writer** (port `compact.rs`; add `boomphf` + `bincode` deps) with a key
   fingerprint; re-solve → `gobblet.ctb`.
3. Fold in: dump the 40,711 unreachable keys; brute-sample the 23,943 mismatches.
4. Publish `gobblet.ctb` as a GitHub release asset (git-ignored, like dobutsu).

**Phase B — Railway probe** (REUSE `v2/gobblet-api/`, which already has axum + tokio +
tower-http CORS and a `/lookup/batch` endpoint matching the frontend exactly:
`{positions: Vec<String>}` → `{evaluations: Vec<Option<i8>>}`).
5. Swap its tablebase backend from **SQLite** to the **MPH `.ctb`**: load `gobblet.ctb`
   (deserialize the boomphf MPH + the n-byte DTM array), and replace `lookup(canonical) ->
   Option<i8>` with: `Board::from_u64(key)`; if `check_winner` is Some → return that winner
   (terminal); else `mph.try_hash(key)?` → DTM → `to_abs` → Option<i8>. Optionally add a
   parallel `dtm: Vec<Option<i32>>` to the response (backward-compatible; enables "win in N"
   once the frontend reads it). Drop the `rusqlite` dep; add `boomphf`/`bincode`. Keep the CORS
   layer (allow brianhliou.com). Strip the deprecated game-logic endpoints.
6. `Dockerfile` (mirror dobutsu): build `gobblet-api`, `ADD` `gobblet.ctb` from a GitHub Release,
   run on `$PORT`. Deploy to Railway.

> **B CODE DONE + tested locally.** `gobblet-api` loads `gobblet.ctb` (GOBCTB01: header + bincode
> `boomphf` MPH + n × i8 DTM) and serves `/lookup/batch` (+ `/api/lookup/batch`) + `/health`,
> returning `{evaluations, dtm}`. Verified vs the real .ctb: init → P1-win DTM 13; the two
> mismatch keys → correct. **Abuse guards** (zero cost for real users; only fire on hostile
> input): batch cap 1024 → 400; body limit 256 KB; request timeout 10 s; CORS via `CORS_ORIGIN`
> env (comma-list; default any). `v2/Dockerfile` written (`CTB_URL` build-arg).
> **Remaining (deploy — your Railway):** publish `gobblet.ctb` as a GitHub Release; create the
> Railway service from `v2/Dockerfile` (root dir `v2/`); set `VITE_API_URL` → Railway URL and
> `CORS_ORIGIN`; **Cloudflare in front of gobblet AND dobutsu** as the shared edge rate-limit/DDoS
> layer (dobutsu's `serve.py` is single-threaded, so it benefits more).

**Phase C — repoint + public-readiness cleanup**
7. Set `VITE_API_URL` → Railway URL; redeploy Vercel.
8. `git rm v2/frontend-wasm/api/tablebase.bin`; drop the `functions.includeFiles` entry from
   `vercel.json` and delete `api/lookup/batch.ts` (the embedded-TB function). Serve the corrected
   `gobblet.ctb` from a **GitHub Release** (Railway `ADD`s it at build time, like dobutsu).
   Optional before flipping public: `git-filter-repo` to purge the old 178 MB LFS blob from history.

**Phase D — writing** (deferred until the viewer is correct): decide new article vs. update the
existing one. Out of scope until A–C land.

## How to reproduce the solve

```sh
cd v2/gobblet-solver
cargo run --release --bin retro -- --selftest                          # validate KeyMap
cargo run --release --bin retro -- --inspect-tb ../frontend-wasm/api/tablebase.bin
cargo run --release --bin retro -- --verify ../frontend-wasm/api/tablebase.bin   # ~30 min, peak ~12 GB
```
