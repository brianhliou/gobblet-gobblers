# Game Logic Testing

This document specifies all test cases required to verify game logic correctness. It serves as the contract for V1/V2 parity.

## Test Categories

1. **Basic Operations** - Board state, piece placement, reserves
2. **Movement** - Board moves, gobbling
3. **Win Detection** - All 8 lines, edge cases
4. **Reveal Rule** - The complex cases
5. **Encoding** - Bit representation correctness
6. **Symmetry** - Canonicalization
7. **Parity** - V1/V2 produce identical results

---

## Category 1: Basic Operations

### 1.1 Initial State

```
Given: New game
Expect:
  - All 9 cells empty
  - P1 reserves: S=2, M=2, L=2
  - P2 reserves: S=2, M=2, L=2
  - Current player: P1
  - Legal moves: 27 (3 sizes × 9 positions)
```

### 1.2 Reserve Placement

```
Given: New game
Action: P1 places Small at (0,0)
Expect:
  - Cell (0,0) shows P1 Small on top
  - P1 reserves: S=1, M=2, L=2
  - Current player: P2
```

### 1.3 Reserve Exhaustion

```
Given: P1 has placed both Small pieces
Action: Generate P1's legal moves
Expect: No Small placement moves available
```

---

## Category 2: Movement

### 2.1 Board Move (Simple)

```
Given: P1 Large at (0,0), otherwise empty, P1 to move
Action: P1 moves from (0,0) to (1,1)
Expect:
  - Cell (0,0) empty
  - Cell (1,1) shows P1 Large
  - Current player: P2
```

### 2.2 Gobbling Opponent

```
Given: P2 Small at (1,1), P1 to move
Action: P1 places Large at (1,1)
Expect:
  - Cell (1,1) shows P1 Large on top
  - Stack at (1,1): [P2 Small, P1 Large]
  - P2 Small is hidden but still exists
```

### 2.3 Gobbling Self

```
Given: P1 Small at (1,1), P1 to move
Action: P1 places Large at (1,1)
Expect:
  - Cell (1,1) shows P1 Large on top
  - Stack at (1,1): [P1 Small, P1 Large]
```

### 2.4 Cannot Gobble Equal Size

```
Given: P1 Medium at (1,1), P2 to move
Action: Generate P2's legal moves
Expect: No Medium placement at (1,1) allowed
```

### 2.5 Cannot Gobble Larger

```
Given: P1 Large at (1,1), P2 to move
Action: Generate P2's legal moves
Expect: No moves to (1,1) allowed (nothing gobbles Large)
```

### 2.6 Revealing Piece Underneath

```
Given: Stack at (1,1) = [P2 Small, P1 Large], P1 to move
Action: P1 moves Large from (1,1) to (0,0)
Expect:
  - Cell (1,1) shows P2 Small (revealed)
  - Cell (0,0) shows P1 Large
```

---

## Category 3: Win Detection

### 3.1 Row Win

```
Given: P1 pieces visible at (0,0), (0,1), (0,2)
Expect: P1 wins (row 0)
```

### 3.2 Column Win

```
Given: P2 pieces visible at (0,0), (1,0), (2,0)
Expect: P2 wins (column 0)
```

### 3.3 Diagonal Win

```
Given: P1 pieces visible at (0,0), (1,1), (2,2)
Expect: P1 wins (main diagonal)
```

### 3.4 Anti-Diagonal Win

```
Given: P1 pieces visible at (0,2), (1,1), (2,0)
Expect: P1 wins (anti-diagonal)
```

### 3.5 Hidden Piece Doesn't Count

```
Given:
  - P1 visible at (0,0), (0,1)
  - Stack at (0,2) = [P1 Small, P2 Large]  (P1 hidden under P2)
Expect: No winner (P1 only has 2 visible in row 0)
```

### 3.6 Win On Move

```
Given: P1 visible at (0,0), (0,1), P1 to move
Action: P1 places any piece at (0,2)
Expect: Game ends, P1 wins
```

### 3.7 Multiple Lines (Still One Winner)

```
Given: P1 visible at (0,0), (0,1), (0,2), (1,0), (2,0)
Expect: P1 wins (doesn't matter that two lines exist)
```

---

## Category 4: Reveal Rule

This is the most complex category. The reveal rule states:
- When you lift a piece, the board state changes BEFORE you place
- If lifting reveals opponent's winning line, you MUST gobble into that line
- You cannot place back on the same square you lifted from
- If no valid gobble target exists, the move is illegal

### 4.1 Basic Reveal - Restricted Destinations

```
Given:
  - P2 visible at (0,0), (0,1)
  - Stack at (0,2) = [P2 Small, P1 Large]
  - P1 to move

Action: Generate moves from (0,2)

Expect:
  - Lifting P1 Large reveals P2 Small → P2 wins row 0
  - Valid destinations: only (0,0), (0,1) (squares in winning line P1 can gobble)
  - Invalid: (0,2) (same square), (1,0), (1,1), etc. (not in line)
```

### 4.2 Reveal - Blocking Is Sufficient

```
Given: Setup from 4.1
Action: P1 moves Large from (0,2) to (0,1)

Expect:
  - Move is legal (gobbles into winning line)
  - Game continues (ONGOING, not P1 win or P2 win)
  - P1 did NOT need to create own winning line
```

### 4.3 Reveal - No Save Possible (Piece Too Small)

```
Given:
  - Stack at (0,0) = [P2 Large]
  - Stack at (0,1) = [P2 Large]
  - Stack at (0,2) = [P2 Large, P1 Small]
  - P1 to move

Action: Generate moves from (0,2)

Expect: Empty list (Small cannot gobble any Large in the line)
```

### 4.4 Same-Square Restriction

```
Given:
  - P2 visible at (0,0), (0,1)
  - Stack at (0,2) = [P2 Small, P1 Large]  (P1 Large is top, P2 Small underneath)
  - P1 to move

Action: Can P1 move from (0,2) to (0,2)?

Expect: NO - even though (0,2) is in the winning line, cannot place back on same square
```

### 4.5 Reveal - Would Create Own Win But Also Reveals Opponent Win

```
Given:
  - P2 visible at (0,0), (0,1)
  - Stack at (0,2) = [P2 Small, P1 Large]
  - P1 visible at (1,1), (2,0)  (P1 needs (2,2) for diagonal win)
  - P1 to move

Action: Can P1 move Large from (0,2) to (2,2)?

Expect: NO
  - Lifting reveals P2 row 0 win
  - (2,2) is not in P2's winning line
  - Even though P1 would win diagonally, the reveal happens first
```

### 4.6 Reveal - Multiple Winning Lines Revealed

```
Given:
  - P2 visible at (0,0), (0,1), (1,0)
  - Stack at (0,2) = [P2 Medium, P1 Large]  (covers row win)
  - Stack at (2,0) = [P2 Medium, P1 Large]  (covers column win)
  - P1 to move

Action: Generate moves from (0,2)

Question: Which winning line must P1 block?

Expect: P1 must block row 0 (the line revealed by lifting from (0,2))
  - Valid destinations: (0,0), (0,1) (in row 0, can gobble)
  - Column 0 winning line is not revealed by this lift
```

### 4.7 Zugzwang From Reveal

```
Given:
  - P2 visible at (0,0), (0,1)
  - Stack at (0,2) = [P2 Large, P1 Small]
  - P1 has no other pieces on board
  - P1 has no pieces in reserve
  - P1 to move

Expect:
  - P1 must move Small from (0,2)
  - But Small cannot gobble any position in row 0 (all have P2 pieces)
  - P1 has no legal moves → P1 loses (zugzwang)
```

### 4.8 Reserve Placement During Reveal Situation

```
Given:
  - P2 visible at (0,0), (0,1)
  - Stack at (0,2) = [P2 Small, P1 Large]
  - P1 has pieces in reserve
  - P1 to move

Action: Can P1 place a reserve piece instead of moving the Large?

Expect: YES - reserve placements are always legal (don't trigger reveal)
  - P1 can place anywhere valid from reserve
  - The reveal rule only applies to BOARD MOVES (lifting a piece)
```

---

## Category 5: Encoding

### 5.1 Empty Board Encoding

```
Given: New game, P1 to move
Expect: Encoding is 0 (all bits zero except possibly player bit)
```

### 5.2 Single Piece Encoding

```
Given: P1 Small at (0,0), P2 to move
Expect:
  - Cell 0, layer 0 (bottom) = 0b01 (P1)
  - Player bit = 1 (P2's turn)
  - Exact encoding: verify bit pattern
```

### 5.3 Stack Encoding

```
Given: Stack at (0,0) = [P2 Small, P1 Medium, P2 Large]
Expect:
  - Cell 0, layer 0 = 0b10 (P2 Small)
  - Cell 0, layer 1 = 0b01 (P1 Medium)
  - Cell 0, layer 2 = 0b10 (P2 Large)
```

### 5.4 Encode-Decode Roundtrip

```
Given: Any valid board state
Action: encode(state) → bits → decode(bits) → state'
Expect: state == state' (exact equality)
```

### 5.5 Full Stack at Every Cell

```
Given: All 9 cells have 3 pieces stacked
Action: Encode and decode
Expect: Roundtrip preserves all pieces
```

---

## Category 6: Symmetry

### 6.1 Rotation 90°

```
Given: P1 piece at (0,0)
Action: Rotate 90° clockwise
Expect: Piece now at (0,2)
```

### 6.2 All Corners Same Canonical

```
Given: Boards with single P1 Small at (0,0), (0,2), (2,0), (2,2)
Expect: All have same canonical encoding
```

### 6.3 Center Is Invariant

```
Given: P1 piece at (1,1) only
Action: Apply all 8 D4 transforms
Expect: All produce same encoding (center is fixed point)
```

### 6.4 Canonical Is Minimum

```
Given: Any board state
Action: Apply all 8 transforms, compute canonical
Expect: canonical == min(all 8 transforms)
```

### 6.5 Canonical Is Idempotent

```
Given: Any canonical encoding c
Action: decode(c), apply all 8 transforms, take minimum
Expect: Result equals c
```

---

## Category 7: Parity (V1 vs V2)

### 7.1 Encoding Match

```
Given: Same game state in V1 and V2
Expect: encode_v1(state) == encode_v2(state)
```

### 7.2 Move Generation Match

```
Given: Same position in V1 and V2
Action: Generate legal moves
Expect: Same set of moves (may be different order)
```

### 7.3 Game Result Match

```
Given: Same position
Action: Apply same sequence of moves
Expect: Same game result (ONGOING, P1_WINS, P2_WINS, DRAW)
```

### 7.4 Small Game Tree Comparison

```
Given: Initial position
Action: BFS to depth 5, record all reachable positions
Expect: V1 and V2 produce identical sets of canonical encodings
```

### 7.5 Solver Output Match

```
Given: A set of positions with known outcomes from V1 solver
Action: V2 solver computes outcomes
Expect: Outcomes match exactly
```

---

## Edge Cases Checklist

- [ ] Empty board
- [ ] Full board (all pieces placed)
- [ ] All pieces stacked on one cell
- [ ] Zugzwang (no legal moves)
- [ ] Threefold repetition (cycle)
- [ ] Reveal with multiple possible winning lines
- [ ] Reveal where only move is same-square (impossible)
- [ ] Reserve placement vs board move priority
- [ ] Win created by gobbling (piece on top completes line)
- [ ] Win prevented by gobbling (break opponent's line)

---

## Test Execution Approaches

### Automated (Unit Tests)

```rust
#[test]
fn test_initial_state_27_moves() {
    let board = Board::new();
    assert_eq!(board.legal_moves().len(), 27);
}
```

### Property-Based (Fuzzing)

```rust
#[test]
fn fuzz_apply_undo_roundtrip() {
    // Random boards, random moves, verify undo restores state
}
```

### Parity Testing

```rust
#[test]
fn compare_with_v1() {
    // Load V1 test positions, verify V2 produces same results
}
```

### Manual Testing

For visual inspection of complex scenarios, especially reveal rule edge cases.

---

## Test Data

### Known Positions from V1

We can export specific positions from V1 for V2 to verify:

```python
# V1: Export test positions
positions = [
    (canonical, expected_moves, expected_winner),
    ...
]
```

### Generated Positions

Use a small game tree (depth 5-6) to generate positions for parity testing.

---

## V2 Rust Test Coverage

**106 tests** covering all categories. Run: `cd v2/gobblet-core && cargo test`

### Coverage by Category

| Spec ID | Description | V2 Test(s) | Status |
|---------|-------------|------------|--------|
| **1. Basic Operations** ||||
| 1.1 | Initial State | `test_board_new`, `test_initial_moves_count`, `test_reserves_initial` | ✓ |
| 1.2 | Reserve Placement | `test_reserves_after_placement`, `test_apply_place_move` | ✓ |
| 1.3 | Reserve Exhaustion | `test_exhausted_reserves` | ✓ |
| **2. Movement** ||||
| 2.1 | Board Move (Simple) | `test_apply_slide_move`, `test_board_moves` | ✓ |
| 2.2 | Gobbling Opponent | `test_apply_gobble_move`, `test_gobble_moves` | ✓ |
| 2.3 | Gobbling Self | `test_self_gobble` | ✓ |
| 2.4 | Cannot Gobble Equal | `test_cannot_gobble_equal_or_larger` | ✓ |
| 2.5 | Cannot Gobble Larger | `test_cannot_gobble_equal_or_larger` | ✓ |
| 2.6 | Reveal Piece Underneath | `test_apply_slide_reveals_piece` | ✓ |
| **3. Win Detection** ||||
| 3.1 | Row Win | `test_horizontal_win` | ✓ |
| 3.2 | Column Win | `test_vertical_win` | ✓ |
| 3.3 | Diagonal Win | `test_diagonal_win` | ✓ |
| 3.4 | Anti-Diagonal Win | `test_anti_diagonal_win` | ✓ |
| 3.5 | Hidden Piece No Count | `test_hidden_piece_doesnt_count` | ✓ |
| 3.6 | Win On Move | `test_win_on_move` | ✓ |
| 3.7 | Multiple Lines | `test_multiple_winning_lines` | ✓ |
| **4. Reveal Rule** ||||
| 4.1 | Restricted Destinations | `test_reveal_basic_restricted_destinations` | ✓ |
| 4.2 | Blocking Sufficient | `test_reveal_blocking_is_sufficient` | ✓ |
| 4.3 | No Save Possible | `test_reveal_no_save_possible` | ✓ |
| 4.4 | Same-Square Restriction | `test_same_square_restriction` | ✓ |
| 4.5 | Own Win + Opponent Reveal | `test_reveal_own_win_blocked_by_opponent` | ✓ |
| 4.6 | Multiple Lines Revealed | `test_reveal_multiple_pieces_one_restricted` | ✓ |
| 4.7 | Zugzwang From Reveal | `test_zugzwang_from_reveal` | ✓ |
| 4.8 | Reserve During Reveal | `test_reveal_reserve_placements_still_legal` | ✓ |
| **5. Encoding** ||||
| 5.1 | Empty Board | `test_board_new` | ✓ |
| 5.2 | Single Piece | `test_board_cell_roundtrip`, `test_board_top_piece` | ✓ |
| 5.3 | Stack Encoding | `test_push_pop_roundtrip`, `test_gobbled_pieces_still_count` | ✓ |
| 5.4 | Encode-Decode Roundtrip | `test_board_cell_roundtrip` | ✓ |
| 5.5 | Full Stack Every Cell | `test_full_stacks_encoding` | ✓ |
| **6. Symmetry** ||||
| 6.1 | Rotation 90° | `test_rotate_90` | ✓ |
| 6.2 | All Corners Same | `test_all_corners_same_canonical` | ✓ |
| 6.3 | Center Invariant | `test_center_invariant` | ✓ |
| 6.4 | Canonical Is Minimum | `test_canonical_is_minimum` | ✓ |
| 6.5 | Canonical Idempotent | `test_canonical_idempotent` | ✓ |
| **7. Parity (V1 vs V2)** ||||
| 7.1-7.5 | V1/V2 Match | *Deferred to parity milestone* | - |

### Edge Cases

| Edge Case | V2 Test(s) | Status |
|-----------|------------|--------|
| Empty board | `test_board_new` | ✓ |
| Full board | `test_full_board` | ✓ |
| All pieces stacked on one cell | `test_all_pieces_one_cell` | ✓ |
| Zugzwang (no legal moves) | `test_zugzwang_from_reveal` | ✓ |
| Threefold repetition | *Not implemented* | - |
| Win created by gobbling | `test_win_by_gobble` | ✓ |
| Win prevented by gobbling | `test_reveal_blocking_is_sufficient` | ✓ |

---

*Document created: 2024-12-19*
*V2 coverage updated: 2024-12-19*
