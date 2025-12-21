# Gobblet Gobblers Rules

## Setup
- 3x3 board, starts empty
- 2 players, each with 6 pieces in reserve: 2 small, 2 medium, 2 large
- Sizes: large > medium > small

## Turns
- On your turn, either:
  - Place a piece from your reserve onto the board
  - Move a piece you control that's visible on the board
- Larger pieces can "gobble" (cover) smaller pieces (yours or opponent's)
- You cannot pass; must make a move if any legal move exists

## Winning
- Get 3 of your pieces visible in a row (horizontal, vertical, or diagonal)
- Win is checked immediately when a line becomes visible

## The "Reveal" Rule (Critical)
- When you lift a piece to move it, what's underneath is revealed
- If lifting reveals opponent's winning line: you lose UNLESS you can gobble
  one of the pieces in that winning line with the piece you're holding
- **Blocking is sufficient** - you do NOT need to create your own winning line
- The term "hail mary" refers to the desperate blocking move, not a requirement to also win
- If you successfully gobble into the winning line, the game continues normally
- If you cannot legally land on that line, you lose immediately
- If your move creates your win BUT also reveals opponent's win, opponent wins
  (lift happens before place)

Example of valid blocking (game continues):
```
After move 5:
  0   1   2
0 2L  ..  2S    (P2 Large on top of P1 Small at (0,0))
1 1M  ..  ..    (P1 Medium)
2 1M  ..  ..    (P1 Medium)

P1 has 2/3 of column 0. P2 lifts L from (0,0) → reveals P1 S → P1 wins column 0!
P2 MUST gobble into column 0 to survive.
Move: (0,0)→(1,0) - P2 L gobbles P1 M at (1,0)
Result: Column 0 is now P1 S, P2 L, P1 M - no winner. Game continues.
```

## No Same-Square Moves (Important Edge Case)
- Once you lift a piece, you CANNOT place it back on the same square
- This is true even in a hail mary situation
- If lifting reveals opponent's win and the only "valid" gobble target is the
  square you just picked up from, you have NO legal moves with that piece
- Example scenario:
  ```
  Board after move 5:
    0   1   2
  0 2M  ..  2S    (2M gobbled 1S at (0,0))
  1 1M  ..  ..
  2 1L  ..  ..

  P1 has column 0 except (0,0). If P2 lifts M from (0,0), P1 wins!
  P2's M could gobble (0,0)'s revealed 1S, but that's the same square.
  P2 cannot gobble (1,0) M or (2,0) L. So lifting that M = instant loss.
  ```
- Source: Official rules state "you can't return the piece to its starting location"

## Zugzwang (No Legal Moves)
- If a player has no legal moves on their turn, they lose immediately
- This can happen when all moveable pieces would reveal opponent's winning line
  with no valid hail mary destinations

## Draw
- Threefold repetition of the same board position = draw
- No other draw conditions (full board just means you must move existing pieces)
