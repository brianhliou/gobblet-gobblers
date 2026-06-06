//! Ground-truth retrograde solver for Gobblet Gobblers ("dumb way first").
//!
//! Replaces the forward DFS + transposition-table minimax (GHI bug — see
//! docs/game_tree_analysis.md and docs/retrograde_correction.md) with a two-phase
//! retrograde analysis that is correct for a game with cycles:
//!
//!   Phase 1 — enumerate every NON-TERMINAL canonical position reachable from a
//!             start (BFS; game-over boards are leaves, not nodes — the viewer
//!             detects them locally via check_winner).
//!   Phase 2 — fill distance-to-result by backward induction to a fixpoint.
//!
//! Values are signed DTM (side-to-move view): +d = STM wins in d plies, -d = STM
//! loses in d plies, 0 = DRAW. Draws are NOT detected by repetition — they are the
//! cycle-bound residue the fixpoint never proves won or lost (left 0 at convergence).
//!
//! The fixpoint is **parallel Jacobi**: each round reads prior-round values, so a
//! position resolves exactly in its DTM round and the first decided loss-child it
//! sees is its minimum-depth winning move => exact (minimal) win-distance.
//!
//! Memory: custom open-addressing key->id table (~6 GB at ~0.72 load over the ~371M
//! non-terminal positions) + Vec<i16> values + key worklist. Peak ~10.5 GB.
//!
//!   cargo run --release --bin retro -- --selftest
//!   cargo run --release --bin retro -- --save-ctb gobblet.ctb   # cut the deployable tablebase
//!   cargo run --release --bin retro -- --verify   gobblet.ctb   # re-check a .ctb
//!   cargo run --release --bin retro -- --inspect-tb <raw.bin>

use std::collections::VecDeque;
use std::env;
use std::io::Write;
use std::time::Instant;

use boomphf::Mphf;
use gobblet_core::{Board, Move, Player};
use gobblet_solver::movegen::MoveGenerator;
use rayon::prelude::*;

const UNKNOWN: i16 = 0; // fixpoint sentinel; also the FINAL value of a DRAW

// ---- custom open-addressing key->id table (linear probing, ~0.72 load) ----
// keys 8B + ids 4B per slot, no power-of-two waste. Key 0 (the initial position's
// canonical key) is special-cased so 0 can serve as the empty slot.
struct KeyMap {
    keys: Vec<u64>,
    ids: Vec<u32>,
    cap: u64,
    len: u32,
    max_fill: u32,
    zero_id: i64, // id of key 0, or -1 if absent
}

impl KeyMap {
    fn with_capacity(est: usize) -> Self {
        let cap = ((est as f64 * 1.28) as u64).max(16);
        KeyMap {
            keys: vec![0u64; cap as usize],
            ids: vec![0u32; cap as usize],
            cap,
            len: 0,
            max_fill: (cap * 92 / 100) as u32,
            zero_id: -1,
        }
    }

    #[inline]
    fn home(&self, key: u64) -> usize {
        (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) % self.cap) as usize
    }

    #[inline]
    fn get(&self, key: u64) -> Option<u32> {
        if key == 0 {
            return if self.zero_id >= 0 { Some(self.zero_id as u32) } else { None };
        }
        let mut s = self.home(key);
        loop {
            let k = self.keys[s];
            if k == key {
                return Some(self.ids[s]);
            }
            if k == 0 {
                return None;
            }
            s += 1;
            if s as u64 == self.cap {
                s = 0;
            }
        }
    }

    /// Insert `key` if absent, assigning `new_id`. Returns (id, inserted?).
    #[inline]
    fn get_or_insert(&mut self, key: u64, new_id: u32) -> (u32, bool) {
        if key == 0 {
            if self.zero_id >= 0 {
                return (self.zero_id as u32, false);
            }
            self.zero_id = new_id as i64;
            self.len += 1;
            return (new_id, true);
        }
        if self.len >= self.max_fill {
            panic!("KeyMap overfull (len {} cap {}): raise the est", self.len, self.cap);
        }
        let mut s = self.home(key);
        loop {
            let k = self.keys[s];
            if k == key {
                return (self.ids[s], false);
            }
            if k == 0 {
                self.keys[s] = key;
                self.ids[s] = new_id;
                self.len += 1;
                return (new_id, true);
            }
            s += 1;
            if s as u64 == self.cap {
                s = 0;
            }
        }
    }

    fn len(&self) -> usize {
        self.len as usize
    }

    /// Visit every (key, id) entry.
    fn for_each<F: FnMut(u64, u32)>(&self, mut f: F) {
        if self.zero_id >= 0 {
            f(0, self.zero_id as u32);
        }
        for s in 0..self.keys.len() {
            let k = self.keys[s];
            if k != 0 {
                f(k, self.ids[s]);
            }
        }
    }
}

struct Solved {
    table: KeyMap,     // canonical key -> dense id (NON-terminal positions only)
    values: Vec<i16>,  // dense id -> signed DTM (side-to-move view)
}

/// Child board after a move (Board is Copy, so no undo bookkeeping).
#[inline]
fn child_of(b: &Board, m: Move) -> Board {
    let mut c = *b;
    c.apply(m);
    c
}

/// Absolute outcome (1 = P1 win, 0 = draw, -1 = P2 win) from a STM DTM value.
fn to_abs(b: &Board, v: i16) -> i8 {
    match v.signum() {
        1 => {
            if b.current_player() == Player::One {
                1
            } else {
                -1
            }
        }
        -1 => {
            if b.current_player() == Player::One {
                -1
            } else {
                1
            }
        }
        _ => 0,
    }
}

/// Enumerate non-terminal canonical positions reachable from `init` and fill
/// signed DTM by retrograde fixpoint. None if enumeration exceeds `max_positions`.
fn solve_reachable(
    init: Board,
    est: usize,
    max_positions: Option<usize>,
    verbose: bool,
    t0: Instant,
) -> Option<Solved> {
    // --- Phase 1: BFS flood fill, EXCLUDING game-over children ---
    let seed = init.canonical();
    let mut table = KeyMap::with_capacity(est);
    let mut q: VecDeque<u64> = VecDeque::new();
    table.get_or_insert(seed, 0);
    q.push_back(seed);

    while let Some(k) = q.pop_front() {
        let b = Board::from_u64(k); // non-terminal by construction
        let mut g = MoveGenerator::new(&b);
        while let Some(m) = g.next(&b) {
            let c = child_of(&b, m);
            if c.check_winner().is_some() {
                continue; // game-over board: a leaf, not a node
            }
            let ck = c.canonical();
            let nid = table.len() as u32;
            let (_id, is_new) = table.get_or_insert(ck, nid);
            if is_new {
                q.push_back(ck);
                if let Some(cap) = max_positions {
                    if table.len() > cap {
                        if verbose {
                            let secs = t0.elapsed().as_secs_f64().max(1e-9);
                            eprintln!(
                                "[{:?}] hit cap {} positions ({:.0}/s) — aborting enumeration",
                                t0.elapsed(),
                                table.len(),
                                table.len() as f64 / secs
                            );
                        }
                        return None;
                    }
                }
            }
        }
    }
    drop(q);
    let n = table.len();
    if verbose {
        eprintln!("[{:?}] enumerated {n} non-terminal canonical positions", t0.elapsed());
    }

    // --- Phase 2a: build the worklist (serial). All nodes are non-terminal; a
    //     non-terminal with no legal move is a loss (rare/none). Positions with an
    //     immediate winning move are NOT pre-seeded — they resolve in round 1. ---
    let mut values = vec![UNKNOWN; n];
    let mut unknown: Vec<u64> = Vec::new();
    let mut no_move = 0u64;
    table.for_each(|key, id| {
        let b = Board::from_u64(key);
        if MoveGenerator::new(&b).next(&b).is_none() {
            values[id as usize] = -1; // no legal move => STM loses
            no_move += 1;
        } else {
            unknown.push(key);
        }
    });
    if verbose {
        eprintln!(
            "[{:?}] worklist: no-move {no_move}, unknown {}",
            t0.elapsed(),
            unknown.len()
        );
    }

    // --- Phase 2b: PARALLEL JACOBI fixpoint (exact DTM) ---
    // Per move from p: a winning move => p wins in 1; a losing move (reveal) => a
    // child that wins in 0 (p loses in 1); a continuing move => the non-terminal
    // child's DTM. p = min-depth win if any; else loss = slowest losing line; else
    // (a child still UNKNOWN) defer. Reads see prior-round values (no mid-round
    // writes) => decisions are order-independent and win-depths are minimal.
    let mut round = 0u32;
    loop {
        round += 1;
        let decisions: Vec<(u32, i16)> = unknown
            .par_iter()
            .filter_map(|&key| {
                let b = Board::from_u64(key);
                let mut imm_win = false;
                let mut best_win: Option<i16> = None; // min child-loss-depth
                let mut worst_loss: i16 = -1; // max child-win-depth; -1 = none seen
                let mut any_unknown = false;
                let mut g = MoveGenerator::new(&b);
                while let Some(m) = g.next(&b) {
                    let c = child_of(&b, m);
                    match c.check_winner() {
                        Some(w) if w == b.current_player() => {
                            imm_win = true;
                            break;
                        }
                        Some(_) => worst_loss = worst_loss.max(0), // losing move: lose in 1
                        None => {
                            let cv = values[table.get(c.canonical()).unwrap() as usize];
                            if cv == UNKNOWN {
                                any_unknown = true;
                            } else if cv < 0 {
                                let d = -cv;
                                best_win = Some(best_win.map_or(d, |x| x.min(d)));
                            } else {
                                worst_loss = worst_loss.max(cv);
                            }
                        }
                    }
                }
                let my_id = table.get(key).unwrap();
                if imm_win {
                    Some((my_id, 1))
                } else if let Some(d) = best_win {
                    Some((my_id, d + 1))
                } else if any_unknown {
                    None // defer
                } else if worst_loss >= 0 {
                    Some((my_id, -(worst_loss + 1)))
                } else {
                    Some((my_id, -1)) // degenerate no-move (shouldn't reach here)
                }
            })
            .collect();
        let d = decisions.len();
        for &(id, v) in &decisions {
            values[id as usize] = v; // disjoint ids; Jacobi (applied after the round)
        }
        unknown.retain(|&key| values[table.get(key).unwrap() as usize] == UNKNOWN);
        if verbose && (round % 5 == 1 || d == 0) {
            eprintln!(
                "[{:?}] round {round}: decided {d}, remaining {}",
                t0.elapsed(),
                unknown.len()
            );
        }
        if d == 0 {
            break;
        }
    }
    Some(Solved { table, values })
}

/// Counterfactual solve: like `solve_reachable` but with a pluggable move set,
/// so we can drop the reveal rule (`legal_moves_simple`) or covering
/// (`moves_nostack`) and measure what each rule contributes. Slower (Vec alloc
/// per position) but these are one-offs.
fn solve_reachable_with<G: Fn(&Board) -> Vec<Move> + Sync>(
    init: Board,
    est: usize,
    max_positions: Option<usize>,
    verbose: bool,
    t0: Instant,
    gen_moves: G,
) -> Option<Solved> {
    let seed = init.canonical();
    let mut table = KeyMap::with_capacity(est);
    let mut q: VecDeque<u64> = VecDeque::new();
    table.get_or_insert(seed, 0);
    q.push_back(seed);

    while let Some(k) = q.pop_front() {
        let b = Board::from_u64(k);
        for m in gen_moves(&b) {
            let c = child_of(&b, m);
            if c.check_winner().is_some() {
                continue;
            }
            let ck = c.canonical();
            let nid = table.len() as u32;
            let (_id, is_new) = table.get_or_insert(ck, nid);
            if is_new {
                q.push_back(ck);
                if let Some(cap) = max_positions {
                    if table.len() > cap {
                        if verbose {
                            eprintln!("[{:?}] hit cap {} — aborting", t0.elapsed(), table.len());
                        }
                        return None;
                    }
                }
            }
        }
    }
    drop(q);
    let n = table.len();
    if verbose {
        eprintln!("[{:?}] (no-reveal) enumerated {n} non-terminal positions", t0.elapsed());
    }

    let mut values = vec![UNKNOWN; n];
    let mut unknown: Vec<u64> = Vec::new();
    table.for_each(|key, id| {
        let b = Board::from_u64(key);
        if gen_moves(&b).is_empty() {
            values[id as usize] = -1;
        } else {
            unknown.push(key);
        }
    });

    let mut round = 0u32;
    loop {
        round += 1;
        let decisions: Vec<(u32, i16)> = unknown
            .par_iter()
            .filter_map(|&key| {
                let b = Board::from_u64(key);
                let mut imm_win = false;
                let mut best_win: Option<i16> = None;
                let mut worst_loss: i16 = -1;
                let mut any_unknown = false;
                for m in gen_moves(&b) {
                    let c = child_of(&b, m);
                    match c.check_winner() {
                        Some(w) if w == b.current_player() => {
                            imm_win = true;
                            break;
                        }
                        Some(_) => worst_loss = worst_loss.max(0),
                        None => {
                            let cv = values[table.get(c.canonical()).unwrap() as usize];
                            if cv == UNKNOWN {
                                any_unknown = true;
                            } else if cv < 0 {
                                let d = -cv;
                                best_win = Some(best_win.map_or(d, |x| x.min(d)));
                            } else {
                                worst_loss = worst_loss.max(cv);
                            }
                        }
                    }
                }
                let my_id = table.get(key).unwrap();
                if imm_win {
                    Some((my_id, 1))
                } else if let Some(d) = best_win {
                    Some((my_id, d + 1))
                } else if any_unknown {
                    None
                } else if worst_loss >= 0 {
                    Some((my_id, -(worst_loss + 1)))
                } else {
                    Some((my_id, -1))
                }
            })
            .collect();
        let d = decisions.len();
        for &(id, v) in &decisions {
            values[id as usize] = v;
        }
        unknown.retain(|&key| values[table.get(key).unwrap() as usize] == UNKNOWN);
        if verbose && (round % 5 == 1 || d == 0) {
            eprintln!("[{:?}] (no-reveal) round {round}: decided {d}, remaining {}", t0.elapsed(), unknown.len());
        }
        if d == 0 {
            break;
        }
    }
    Some(Solved { table, values })
}

/// No-covering move set: placements and slides whose destination is an empty
/// cell. With covering disabled the board never stacks, so this is the full
/// legal set for the no-stack counterfactual.
fn moves_nostack(b: &Board) -> Vec<Move> {
    b.legal_moves_simple()
        .into_iter()
        .filter(|m| {
            let to = match m {
                Move::Place { to, .. } => *to,
                Move::Slide { to, .. } => *to,
            };
            b.top_piece(to).is_none()
        })
        .collect()
}

/// Mine the solved (true-game) instance for notable findings.
fn analyze(solved: &Solved, init: Board, t0: Instant) {
    let n = solved.table.len();

    // --- serial pass: outcomes, DTM histograms, draw structure ---
    let (mut p1, mut p2, mut draw) = (0u64, 0u64, 0u64);
    let mut win_hist = vec![0u64; 256];
    let mut loss_hist = vec![0u64; 256];
    let mut max_win = 0i16;
    let mut max_win_key = 0u64;
    let mut draw_placed = [0u64; 13];
    solved.table.for_each(|key, id| {
        let v = solved.values[id as usize];
        let b = Board::from_u64(key);
        match to_abs(&b, v) {
            1 => p1 += 1,
            -1 => p2 += 1,
            _ => draw += 1,
        }
        if v > 0 {
            if (v as usize) < win_hist.len() {
                win_hist[v as usize] += 1;
            }
            if v > max_win {
                max_win = v;
                max_win_key = key;
            }
        } else if v < 0 {
            let d = (-v) as usize;
            if d < loss_hist.len() {
                loss_hist[d] += 1;
            }
        } else {
            let placed = (b.pieces_on_board(Player::One).iter().sum::<u8>()
                + b.pieces_on_board(Player::Two).iter().sum::<u8>()) as usize;
            draw_placed[placed.min(12)] += 1;
        }
    });

    println!("=== FINDINGS (reveal rule, true game) ===");
    println!("non-terminal positions {n}");
    println!("absolute: P1-win {p1}  P2-win {p2}  draw {draw}");
    let root_v = solved.values[solved.table.get(init.canonical()).unwrap() as usize];
    println!(
        "start: {} DTM {root_v}",
        match to_abs(&init, root_v) {
            1 => "P1 wins",
            -1 => "P2 wins",
            _ => "draw",
        }
    );
    println!("WON-BY-DISTANCE (STM win-in-d):");
    for d in 1..win_hist.len() {
        if win_hist[d] > 0 {
            println!("  d={d}: {}", win_hist[d]);
        }
    }
    println!("deepest_win_DTM {max_win}  example_key {max_win_key}");
    let mut max_loss = 0usize;
    for d in 1..loss_hist.len() {
        if loss_hist[d] > 0 {
            max_loss = d;
        }
    }
    println!("deepest_loss_DTM {max_loss}");
    println!("DRAWS-BY-PIECES-PLACED:");
    for k in 0..=12 {
        if draw_placed[k] > 0 {
            println!("  placed={k}: {}", draw_placed[k]);
        }
    }

    // --- opening: start placements ---
    println!("OPENING (start placements):");
    let mut win_by_size = [0u32; 3];
    let mut tot_by_size = [0u32; 3];
    let mut g = MoveGenerator::new(&init);
    while let Some(m) = g.next(&init) {
        let sz = match m {
            Move::Place { size, .. } => size as usize,
            Move::Slide { .. } => continue,
        };
        let c = child_of(&init, m);
        let abs = if let Some(w) = c.check_winner() {
            if w == init.current_player() {
                1
            } else {
                -1
            }
        } else {
            to_abs(&c, solved.values[solved.table.get(c.canonical()).unwrap() as usize])
        };
        tot_by_size[sz] += 1;
        if abs == 1 {
            win_by_size[sz] += 1;
        }
    }
    println!("  P1-winning placements by size [S,M,L]: {:?} of {:?}", win_by_size, tot_by_size);

    // --- only-one-winning-move (parallel over all real keys) ---
    let one_move = |key: u64| -> (u64, u64) {
        let id = solved.table.get(key).unwrap();
        let v = solved.values[id as usize];
        if v <= 0 {
            return (0, 0);
        }
        let b = Board::from_u64(key);
        let mut opt = 0u32;
        let mut g = MoveGenerator::new(&b);
        while let Some(m) = g.next(&b) {
            let c = child_of(&b, m);
            match c.check_winner() {
                Some(w) if w == b.current_player() => {
                    if v == 1 {
                        opt += 1;
                    }
                }
                Some(_) => {}
                None => {
                    let cv = solved.values[solved.table.get(c.canonical()).unwrap() as usize];
                    if cv < 0 && (-cv + 1) == v {
                        opt += 1;
                    }
                }
            }
        }
        (1, if opt == 1 { 1 } else { 0 })
    };
    let (won_cnt, one_opt): (u64, u64) = solved
        .table
        .keys
        .par_iter()
        .map(|&k| if k == 0 { (0, 0) } else { one_move(k) })
        .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let (won_z, one_z) = if solved.table.zero_id >= 0 { one_move(0) } else { (0, 0) };
    let won_cnt = won_cnt + won_z;
    let one_opt = one_opt + one_z;
    println!(
        "ONLY-ONE-WINNING-MOVE: {one_opt} of {won_cnt} won ({:.1}%)",
        100.0 * one_opt as f64 / won_cnt.max(1) as f64
    );

    // --- reveal-rule prevalence (parallel) ---
    let reveal_active = |key: u64| -> u64 {
        let b = Board::from_u64(key);
        if b.legal_moves().len() != b.legal_moves_simple().len() {
            1
        } else {
            0
        }
    };
    let reveal_cnt: u64 = solved.table.keys.par_iter().map(|&k| if k == 0 { 0 } else { reveal_active(k) }).sum::<u64>()
        + if solved.table.zero_id >= 0 { reveal_active(0) } else { 0 };
    println!("REVEAL-RULE-ACTIVE: {reveal_cnt} of {n} ({:.1}%)", 100.0 * reveal_cnt as f64 / n.max(1) as f64);
    println!("[{:?}] findings done", t0.elapsed());
}

/// Independent oracle: depth- and node-bounded, cache-free negamax with canonical
/// 2-fold repetition = draw. Returns the STM outcome SIGN (1 win, -1 loss, 0 draw),
/// or None if it could not resolve within the budget.
fn brute(b: &Board, path: &mut Vec<u64>, depth: u32, budget: &mut u64) -> Option<i16> {
    if let Some(w) = b.check_winner() {
        return Some(if w == b.current_player() { 1 } else { -1 });
    }
    if depth == 0 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    let ck = b.canonical();
    if path.contains(&ck) {
        return Some(0); // canonical repetition => draw
    }
    let mut moves = MoveGenerator::new(b);
    let mut has_move = false;
    path.push(ck);
    let mut saw_draw = false;
    let mut any_none = false;
    let mut win = false;
    while let Some(m) = moves.next(b) {
        has_move = true;
        match brute(&child_of(b, m), path, depth - 1, budget) {
            Some(-1) => {
                win = true;
                break;
            }
            Some(0) => saw_draw = true,
            Some(_) => {}
            None => any_none = true,
        }
    }
    path.pop();
    if !has_move {
        return Some(-1);
    }
    if win {
        Some(1)
    } else if any_none {
        None
    } else if saw_draw {
        Some(0)
    } else {
        Some(-1)
    }
}

/// Cross-check a solved instance against the brute oracle on a strided sample.
fn brute_check(solved: &Solved, k: usize, t0: Instant) {
    let slots = solved.table.keys.len();
    let stride = (slots / k.max(1)).max(1);
    let (mut compared, mut agree, mut mismatch, mut skipped) = (0u64, 0u64, 0u64, 0u64);
    let mut s = 0;
    while s < slots {
        let key = solved.table.keys[s];
        if key != 0 {
            let id = solved.table.ids[s];
            let ours = solved.values[id as usize].signum();
            let b = Board::from_u64(key);
            let mut path = Vec::new();
            let mut budget = 250_000u64;
            match brute(&b, &mut path, 40, &mut budget) {
                Some(v) => {
                    compared += 1;
                    if v == ours {
                        agree += 1;
                    } else {
                        mismatch += 1;
                        if mismatch <= 20 {
                            eprintln!("  MISMATCH key={key} ours={ours} brute={v}");
                        }
                    }
                }
                None => skipped += 1,
            }
        }
        s += stride;
    }
    println!(
        "[{:?}] brute-check: compared {compared}, agree {agree}, mismatch {mismatch}, skipped {skipped}",
        t0.elapsed()
    );
}

fn selftest() {
    let mut m = KeyMap::with_capacity(1_000_000);
    let mut inserted: Vec<(u64, u32)> = Vec::new();
    let mut s = 0x1234_5678_9abc_def0u64;
    for _ in 0..600_000u32 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let key = s;
        let nid = m.len() as u32;
        let (id, is_new) = m.get_or_insert(key, nid);
        if is_new {
            inserted.push((key, id));
        }
    }
    let mut ok = true;
    for &(key, id) in &inserted {
        if m.get(key) != Some(id) {
            ok = false;
            eprintln!("selftest: get({key}) != Some({id})");
            break;
        }
    }
    let mut absent_none = 0;
    for &(key, _) in inserted.iter().take(10_000) {
        if m.get(key ^ 0x8000_0000_0000_0000).is_none() {
            absent_none += 1;
        }
    }
    println!(
        "KeyMap selftest: {} entries, round-trip {}, absent-None {}/10000",
        m.len(),
        if ok { "OK" } else { "FAIL" },
        absent_none
    );
    let mut z = KeyMap::with_capacity(16);
    assert_eq!(z.get(0), None);
    let (id0, new0) = z.get_or_insert(0, 0);
    assert!(new0 && id0 == 0);
    assert_eq!(z.get(0), Some(0));
    let (id0b, new0b) = z.get_or_insert(0, 99);
    assert!(!new0b && id0b == 0);
    println!("KeyMap zero-key: OK");
}

/// Compact tablebase writer (mirrors dobutsu's compact.rs). Consumes the solve:
/// extract (key, DTM) pairs, drop the ~6 GB table, build a boomphf MPH over the
/// keys, and write `.ctb` = MAGIC | n:u64 | mph_len:u64 | mph(bincode) | values
/// (n bytes, i8 DTM in MPH-slot order). The probe looks up by mph.hash(key);
/// terminals are handled by the probe via check_winner before any lookup, so a
/// legitimate query is always in-set (no fingerprint needed).
fn write_ctb(solved: Solved, path: &str, t0: Instant) {
    let n = solved.table.len();
    let mut keys: Vec<u64> = Vec::with_capacity(n);
    let mut vals: Vec<i8> = Vec::with_capacity(n);
    solved.table.for_each(|key, id| {
        let v = solved.values[id as usize];
        assert!((-127..=127).contains(&v), "DTM {v} exceeds i8 range");
        keys.push(key);
        vals.push(v as i8);
    });
    let step = (n / 5000).max(1);
    let mut samples: Vec<(u64, i8)> = Vec::new();
    let mut i = 0;
    while i < n {
        samples.push((keys[i], vals[i]));
        i += step;
    }
    drop(solved); // free the ~6 GB table + values before the MPH build
    eprintln!("[{:?}] extracted {n} (key,DTM) pairs; building MPH...", t0.elapsed());

    let mph = Mphf::new_parallel(1.7, &keys, None);
    eprintln!("[{:?}] built MPH", t0.elapsed());

    let mut slot_vals = vec![0i8; n];
    for j in 0..n {
        slot_vals[mph.hash(&keys[j]) as usize] = vals[j];
    }
    drop(keys);
    drop(vals);

    let mph_bytes = bincode::serialize(&mph).expect("serialize mph");
    let f = std::fs::File::create(path).expect("create ctb");
    let mut w = std::io::BufWriter::new(f);
    w.write_all(b"GOBCTB01").unwrap();
    w.write_all(&(n as u64).to_le_bytes()).unwrap();
    w.write_all(&(mph_bytes.len() as u64).to_le_bytes()).unwrap();
    w.write_all(&mph_bytes).unwrap();
    let vbytes: &[u8] = unsafe { std::slice::from_raw_parts(slot_vals.as_ptr() as *const u8, n) };
    w.write_all(vbytes).unwrap();
    w.flush().unwrap();

    let total = 8 + 8 + 8 + mph_bytes.len() + n;
    eprintln!(
        "[{:?}] wrote {path}: {:.0} MB (mph {:.0} MB, values {:.0} MB)",
        t0.elapsed(),
        total as f64 / 1e6,
        mph_bytes.len() as f64 / 1e6,
        n as f64 / 1e6
    );

    let (mut ok, mut bad) = (0u64, 0u64);
    for &(key, v) in &samples {
        if slot_vals[mph.hash(&key) as usize] == v {
            ok += 1;
        } else {
            bad += 1;
        }
    }
    println!("ctb self-check: {ok} ok, {bad} bad of {} sampled", samples.len());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let t0 = Instant::now();

    if args.iter().any(|a| a == "--selftest") {
        selftest();
        return;
    }

    // Characterize an existing raw tablebase (9-byte records) without solving.
    if let Some(path) = flag_value(&args, "--inspect-tb") {
        let data = std::fs::read(&path).expect("read tb");
        let (mut total, mut noncanon, mut terminal, mut p1, mut d, mut p2) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        let mut placed_hist = [0u64; 13];
        for chunk in data.chunks_exact(9) {
            let key = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let v = chunk[8] as i8;
            total += 1;
            let b = Board::from_u64(key);
            if b.canonical() != key {
                noncanon += 1;
            }
            if b.check_winner().is_some() {
                terminal += 1;
            }
            let placed = (b.pieces_on_board(Player::One).iter().sum::<u8>()
                + b.pieces_on_board(Player::Two).iter().sum::<u8>()) as usize;
            placed_hist[placed.min(12)] += 1;
            match v {
                1 => p1 += 1,
                -1 => p2 += 1,
                _ => d += 1,
            }
        }
        println!("inspect {path}:");
        println!("  total {total}  non-canonical {noncanon}  terminal {terminal}");
        println!("  values: P1 {p1}  draw {d}  P2 {p2}");
        println!("  pieces-placed histogram: {:?}", placed_hist);
        return;
    }

    // Solve the true game and mine it for notable findings.
    if args.iter().any(|a| a == "--findings") {
        let init = Board::new();
        eprintln!("[{:?}] solving true game (reveal rule)...", t0.elapsed());
        let solved = solve_reachable(init, 400_000_000, None, true, t0).expect("solve failed");
        analyze(&solved, init, t0);
        return;
    }

    // Counterfactual: re-solve with the reveal rule removed.
    if args.iter().any(|a| a == "--no-reveal") {
        let est = flag_value(&args, "--est").and_then(|s| s.parse::<usize>().ok()).unwrap_or(700_000_000);
        let init = Board::new();
        eprintln!("[{:?}] solving counterfactual (no reveal rule, est {est})...", t0.elapsed());
        let cf = solve_reachable_with(init, est, None, true, t0, |b| b.legal_moves_simple()).expect("counterfactual solve failed");
        let n2 = cf.table.len();
        let (mut p1, mut p2, mut dr) = (0u64, 0u64, 0u64);
        let mut max_win2 = 0i16;
        cf.table.for_each(|key, id| {
            let v = cf.values[id as usize];
            if v > max_win2 {
                max_win2 = v;
            }
            match to_abs(&Board::from_u64(key), v) {
                1 => p1 += 1,
                -1 => p2 += 1,
                _ => dr += 1,
            }
        });
        let rv = cf.values[cf.table.get(init.canonical()).unwrap() as usize];
        println!("=== COUNTERFACTUAL (no reveal rule) ===");
        println!("non-terminal positions {n2}");
        println!("absolute: P1-win {p1}  P2-win {p2}  draw {dr}");
        println!(
            "start: {} DTM {rv}",
            match to_abs(&init, rv) {
                1 => "P1 wins",
                -1 => "P2 wins",
                _ => "draw",
            }
        );
        println!("deepest_win_DTM {max_win2}");
        println!("[{:?}] counterfactual done", t0.elapsed());
        return;
    }

    // Counterfactual: re-solve with covering disabled (no stacking at all).
    if args.iter().any(|a| a == "--no-stack") {
        let est = flag_value(&args, "--est").and_then(|s| s.parse::<usize>().ok()).unwrap_or(20_000_000);
        let init = Board::new();
        eprintln!("[{:?}] solving counterfactual (no covering, est {est})...", t0.elapsed());
        let cf = solve_reachable_with(init, est, None, true, t0, moves_nostack).expect("no-stack solve failed");
        let n2 = cf.table.len();
        let (mut p1, mut p2, mut dr) = (0u64, 0u64, 0u64);
        let mut max_win2 = 0i16;
        cf.table.for_each(|key, id| {
            let v = cf.values[id as usize];
            if v > max_win2 {
                max_win2 = v;
            }
            match to_abs(&Board::from_u64(key), v) {
                1 => p1 += 1,
                -1 => p2 += 1,
                _ => dr += 1,
            }
        });
        let rv = cf.values[cf.table.get(init.canonical()).unwrap() as usize];
        println!("=== COUNTERFACTUAL (no covering / no stacking) ===");
        println!("non-terminal positions {n2}");
        println!("absolute: P1-win {p1}  P2-win {p2}  draw {dr}");
        println!(
            "start: {} DTM {rv}",
            match to_abs(&init, rv) {
                1 => "P1 wins",
                -1 => "P2 wins",
                _ => "draw",
            }
        );
        println!("deepest_win_DTM {max_win2}");
        println!("[{:?}] no-stack done", t0.elapsed());
        return;
    }

    let from = flag_value(&args, "--from").and_then(|s| parse_u64(&s));
    let max_positions = flag_value(&args, "--max-positions").and_then(|s| s.parse::<usize>().ok());
    let brute_k = flag_value(&args, "--brute-check").and_then(|s| s.parse::<usize>().ok());
    let verify = flag_value(&args, "--verify");

    let init = match from {
        Some(u) => Board::from_u64(u),
        None => Board::new(),
    };
    let est = if from.is_some() { 1 << 20 } else { 400_000_000 };

    let solved = match solve_reachable(init, est, max_positions, true, t0) {
        Some(s) => s,
        None => {
            eprintln!("(aborted at --max-positions cap; timing/RAM probe only)");
            return;
        }
    };

    // ---- report (absolute view + DTM extent) ----
    let (mut p1, mut p2, mut draw) = (0u64, 0u64, 0u64);
    let mut max_win = 0i16;
    let mut max_loss = 0i16;
    solved.table.for_each(|key, id| {
        let v = solved.values[id as usize];
        if v > max_win {
            max_win = v;
        }
        if v < max_loss {
            max_loss = v;
        }
        match to_abs(&Board::from_u64(key), v) {
            1 => p1 += 1,
            -1 => p2 += 1,
            _ => draw += 1,
        }
    });
    let root_id = solved.table.get(init.canonical()).unwrap();
    let root_v = solved.values[root_id as usize];
    println!("=== gobblet-gobblers retrograde (ground truth, DTM) ===");
    println!("non-terminal positions  {}", solved.table.len());
    println!("P1-win {p1}   P2-win {p2}   draw {draw}");
    println!("max win-DTM {max_win}   max loss-DTM {}", -max_loss);
    println!(
        "START POSITION:  {}  (DTM {})",
        match to_abs(&init, root_v) {
            1 => "Player 1 wins",
            -1 => "Player 2 wins",
            _ => "DRAW",
        },
        root_v
    );

    if let Some(k) = brute_k {
        brute_check(&solved, k, t0);
    }

    // ---- cross-check vs the published raw tablebase (terminals excluded here) ----
    if let Some(path) = verify {
        match std::fs::read(&path) {
            Ok(data) => {
                let (mut checked, mut mismatch, mut term_skip, mut nonterm_missing) =
                    (0u64, 0u64, 0u64, 0u64);
                let mut mismatch_keys: Vec<u64> = Vec::new();
                let mut missing_keys: Vec<u64> = Vec::new();
                for chunk in data.chunks_exact(9) {
                    let key = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
                    let theirs = chunk[8] as i8;
                    if let Some(id) = solved.table.get(key) {
                        let ours = to_abs(&Board::from_u64(key), solved.values[id as usize]);
                        checked += 1;
                        if ours != theirs {
                            mismatch += 1;
                            if mismatch_keys.len() < 200 {
                                mismatch_keys.push(key);
                            }
                            if mismatch <= 10 {
                                eprintln!("  verify mismatch key={key}: ours={ours} theirs={theirs}");
                            }
                        }
                    } else if Board::from_u64(key).check_winner().is_some() {
                        term_skip += 1; // terminal: not in our non-terminal set (expected)
                    } else {
                        nonterm_missing += 1;
                        if missing_keys.len() < 40 {
                            missing_keys.push(key);
                        }
                    }
                }
                println!(
                    "verify vs {path}: checked {checked}, mismatches {mismatch}, terminal-skipped {term_skip}, NON-TERMINAL-MISSING {nonterm_missing}"
                );
                if !missing_keys.is_empty() {
                    eprintln!("  non-terminal-missing sample: {:?}", &missing_keys[..missing_keys.len().min(20)]);
                }
                // Brute-sample the mismatches: does the independent oracle back ours or theirs?
                let mut bagree_ours = 0u64;
                let mut bagree_theirs = 0u64;
                let mut bskip = 0u64;
                for &key in mismatch_keys.iter().take(50) {
                    let b = Board::from_u64(key);
                    let our_sign = solved.values[solved.table.get(key).unwrap() as usize].signum();
                    let mut path = Vec::new();
                    let mut bud = 5_000_000u64;
                    match brute(&b, &mut path, 60, &mut bud) {
                        Some(s) if s == our_sign => bagree_ours += 1,
                        Some(_) => bagree_theirs += 1,
                        None => bskip += 1,
                    }
                }
                println!(
                    "  brute-sample of mismatches: agree-with-ours {bagree_ours}, agree-with-theirs {bagree_theirs}, skipped {bskip}"
                );
            }
            Err(e) => eprintln!("verify: could not read {path}: {e}"),
        }
    }

    // Write the compact .ctb last (consumes the solve / frees the big table).
    if let Some(path) = flag_value(&args, "--save-ctb") {
        write_ctb(solved, &path, t0);
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn parse_u64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}
