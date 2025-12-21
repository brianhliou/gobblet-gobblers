# V1 Manual Testing Guide

> **Note**: This document covers V1 Python implementation only. The code is now in `v1/`.
> For V2 Rust, run `cargo test` in `v2/gobblet-core/`.
> See `game_logic_testing.md` for the authoritative test specification.

This document describes the manual testing approach used to verify the core game logic in M1.

## Approach

Tests are executed as Python snippets via the command line. Each test:

1. **Sets up** a specific game scenario (via moves or direct state manipulation)
2. **Executes** an action (make a move, generate legal moves, check state)
3. **Prints** output for visual inspection

This approach allows:
- Fast iteration without test framework overhead
- Construction of arbitrary states (not just those reachable via legal play)
- Visual inspection of board state
- Exploratory testing of edge cases

## Prerequisites

```bash
cd gobblet-gobblers
source .venv/bin/activate
```

## Test Cases

### 1. Initial State

Verify empty board and full reserves.

```python
from gobblet import Game, generate_moves

game = Game()
print(game.state)

moves = game.get_legal_moves()
print(f'Legal moves: {len(moves)}')  # Expected: 27 (3 sizes × 9 positions)
```

**Expected output**:
- All positions show `..` (empty)
- Each player has S:2, M:2, L:2 in reserves
- 27 legal moves available

---

### 2. Basic Move Execution

Place pieces and verify board updates.

```python
from gobblet import Game, Move, Player, Size

game = Game()
game.apply_move(Move(Player.ONE, to_pos=(1, 1), size=Size.LARGE))
print(game.state)
```

**Expected output**:
- `1L` appears at position (1, 1)
- P1 reserves show L:1 (decremented from 2)
- Current player switches to TWO

---

### 3. Gobbling Mechanics

Larger pieces can cover smaller pieces.

```python
from gobblet import Game, Move, Player, Size

game = Game()
game.apply_move(Move(Player.ONE, to_pos=(1, 1), size=Size.SMALL))
game.apply_move(Move(Player.TWO, to_pos=(1, 1), size=Size.LARGE))

print(game.state)
stack = game.state.get_stack((1, 1))
print(f'Stack: {stack}')  # Expected: [1S, 2L]
```

**Expected output**:
- Board shows `2L` at (1, 1) - the visible top piece
- Stack contains both pieces: `[1S, 2L]`

---

### 4. Cannot Gobble Larger/Equal Pieces

Verify gobbling restrictions.

```python
from gobblet import Game, Move, Player, Size

game = Game()
game.apply_move(Move(Player.ONE, to_pos=(1, 1), size=Size.LARGE))

moves = game.get_legal_moves()
moves_to_center = [m for m in moves if m.to_pos == (1, 1)]
print(f'Moves to center: {moves_to_center}')  # Expected: []
```

**Expected output**:
- No moves can target (1, 1) - nothing can gobble a Large

---

### 5. Win Detection

Three in a row triggers win.

```python
from gobblet import Game, GameResult, Move, Player, Size

game = Game()
game.apply_move(Move(Player.ONE, to_pos=(0, 0), size=Size.LARGE))
game.apply_move(Move(Player.TWO, to_pos=(1, 1), size=Size.SMALL))
game.apply_move(Move(Player.ONE, to_pos=(0, 1), size=Size.LARGE))
game.apply_move(Move(Player.TWO, to_pos=(2, 2), size=Size.SMALL))
game.apply_move(Move(Player.ONE, to_pos=(0, 2), size=Size.MEDIUM))

print(f'Result: {game.result}')  # Expected: PLAYER_ONE_WINS
print(f'Game over: {game.is_over()}')  # Expected: True
```

---

### 6. Self-Gobbling

Players can gobble their own pieces.

```python
from gobblet import Game, Move, Player, Size

game = Game()
game.apply_move(Move(Player.ONE, to_pos=(1, 1), size=Size.SMALL))
game.apply_move(Move(Player.TWO, to_pos=(0, 0), size=Size.SMALL))
game.apply_move(Move(Player.ONE, to_pos=(1, 1), size=Size.LARGE))

stack = game.state.get_stack((1, 1))
print(f'Stack: {stack}')  # Expected: [1S, 1L] - both P1's pieces
```

---

### 7. Moving Pieces on Board

Pieces can move from one position to another.

```python
from gobblet import Game, Move, Player, Size

game = Game()
game.apply_move(Move(Player.ONE, to_pos=(0, 0), size=Size.LARGE))
game.apply_move(Move(Player.TWO, to_pos=(2, 2), size=Size.SMALL))

# P1 moves their Large from (0,0) to (1,1)
game.apply_move(Move(Player.ONE, to_pos=(1, 1), from_pos=(0, 0)))

print(game.state)
print(f'(0,0) empty: {game.state.is_empty((0, 0))}')  # Expected: True
```

---

### 8. Reveal Rule - Limited Destinations

When lifting reveals opponent's win, can only gobble into that line.
**Blocking is sufficient** - you don't need to create your own 3-in-a-row.

```python
from gobblet import GameState, Piece, Player, Size, generate_moves

state = GameState()

# P2's winning row, with P1 Large covering (0,2)
state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 0))
state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 1))
state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 2))
state.place_piece(Piece(Player.ONE, Size.LARGE), (0, 2))

moves = generate_moves(state)
moves_from_02 = [m for m in moves if m.from_pos == (0, 2)]
targets = {m.to_pos for m in moves_from_02}
print(f'Legal targets: {targets}')  # Expected: {(0, 0), (0, 1)}
```

**Expected output**:
- Can only move to (0,0) or (0,1) - squares in the winning line where we can gobble
- Cannot move to (0,2) - that's the same square we lifted from (not allowed)
- Cannot move to any square outside the winning line

---

### 8b. Reveal Rule - Valid Blocking (Game Continues)

Demonstrates that blocking is sufficient - no need to create your own winning line.

```python
from gobblet import Game, Move, Player, Size

game = Game()

# Reproduce the scenario:
# 1. S(0,0)  2. L(0,0)  3. M(1,0)  4. S(0,2)  5. M(2,0)  6. (0,0)→(1,0)

game.apply_move(Move(Player.ONE, to_pos=(0, 0), size=Size.SMALL))
game.apply_move(Move(Player.TWO, to_pos=(0, 0), size=Size.LARGE))  # Gobbles P1 S
game.apply_move(Move(Player.ONE, to_pos=(1, 0), size=Size.MEDIUM))
game.apply_move(Move(Player.TWO, to_pos=(0, 2), size=Size.SMALL))
game.apply_move(Move(Player.ONE, to_pos=(2, 0), size=Size.MEDIUM))

# Now P2 lifts L from (0,0), revealing P1 S → P1 would win column 0
# P2 gobbles P1 M at (1,0) to block
game.apply_move(Move(Player.TWO, to_pos=(1, 0), from_pos=(0, 0)))

print(game.state)
print(f'Result: {game.result}')  # Expected: ONGOING (game continues!)
print(f'Current player: {game.state.current_player}')  # Expected: ONE
```

**Expected output**:
- Game result is ONGOING (not a win for either player)
- P2 successfully blocked P1's column by gobbling into it
- Game continues with P1's turn

---

### 9. Reveal Rule - No Save Possible

When piece is too small to gobble into winning line.

```python
from gobblet import GameState, Piece, Player, Size, generate_moves

state = GameState()

# P2's winning row with LARGE pieces, P1 Small on top
state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 0))
state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 1))
state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 2))
state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 2))

moves = generate_moves(state)
moves_from_02 = [m for m in moves if m.from_pos == (0, 2)]
print(f'Legal moves from (0,2): {moves_from_02}')  # Expected: []
```

**Expected output**:
- No legal moves from (0, 2) - Small cannot gobble Large
- That piece is effectively "frozen"

---

### 10. Reveal Rule - Lift Before Place

If your move would create your win BUT reveals opponent's win, opponent wins.

```python
from gobblet import GameState, Piece, Player, Size, generate_moves

state = GameState()

# P2's hidden row under P1's Large at (0,2)
state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 0))
state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 1))
state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 2))
state.place_piece(Piece(Player.ONE, Size.LARGE), (0, 2))

# P1's diagonal needing (2,2) to win
state.place_piece(Piece(Player.ONE, Size.MEDIUM), (1, 1))
state.place_piece(Piece(Player.ONE, Size.MEDIUM), (2, 0))

moves = generate_moves(state)
moves_to_22 = [m for m in moves if m.to_pos == (2, 2) and m.from_pos == (0, 2)]
print(f'Can move to (2,2): {len(moves_to_22) > 0}')  # Expected: False
```

**Expected output**:
- Cannot move Large from (0,2) to (2,2)
- Even though it would complete P1's diagonal, lifting reveals P2's row first
- (2,2) is not in P2's winning line, so it's not a legal destination

---

## Running All Tests

You can combine these into a single script or run them individually:

```bash
python3 -c "
from gobblet import Game, GameState, Move, Piece, Player, Size, generate_moves

# Test 1: Initial state
game = Game()
assert len(game.get_legal_moves()) == 27
print('Test 1 passed: Initial state')

# Test 2: Gobbling
game = Game()
game.apply_move(Move(Player.ONE, to_pos=(1, 1), size=Size.SMALL))
game.apply_move(Move(Player.TWO, to_pos=(1, 1), size=Size.LARGE))
assert len(game.state.get_stack((1, 1))) == 2
print('Test 2 passed: Gobbling')

# ... add more assertions as needed

print('All manual tests passed!')
"
```

## Notes

- These manual tests complement the automated unit tests in `tests/test_game.py`
- Use manual testing for exploratory scenarios and edge case discovery
- Once confident in a scenario, consider adding it as a unit test
