#!/usr/bin/env python3
"""
Export V1 game positions for V2 parity testing.

Generates a comprehensive test set including:
1. Game tree positions (BFS from initial)
2. Specific edge case positions (reveal rule, hail mary, etc.)

Output format: JSON with positions, legal moves, and outcomes.
"""

import json
from collections import deque
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from gobblet.game import GameResult, play_move
from gobblet.moves import generate_moves, move_to_notation
from gobblet.state import GameState
from gobblet.types import Piece, Player, Size

from solver.encoding import canonicalize, encode_state


@dataclass
class PositionData:
    """Data for a single position."""
    canonical: int
    encoding: int  # Raw encoding (before canonicalization)
    current_player: int  # 1 or 2
    legal_moves: list[str]  # Move notations
    winner: int | None  # 1, 2, or None
    depth: int  # Depth from initial position
    description: str  # Human-readable description


def encode_move(move) -> dict:
    """Convert a Move to a JSON-serializable dict."""
    result = {
        "notation": move_to_notation(move),
        "to": list(move.to_pos),
    }
    if move.from_pos is not None:
        result["from"] = list(move.from_pos)
        result["type"] = "slide"
    else:
        result["size"] = move.size.name
        result["type"] = "place"
    return result


def get_winner(state: GameState) -> int | None:
    """Check if there's a winner."""
    if state.check_winner() == Player.ONE:
        return 1
    elif state.check_winner() == Player.TWO:
        return 2
    return None


def place_from_reserve(state: GameState, piece: Piece, pos: tuple[int, int]) -> None:
    """Place a piece from reserve, properly updating both board AND reserves.

    This is the correct way to construct test states. Using place_piece() alone
    creates invalid states where board + reserves > 2 pieces per size.
    """
    state.place_piece(piece, pos)
    state.use_reserve(piece.player, piece.size)


def export_position(state: GameState, depth: int = 0, description: str = "") -> dict:
    """Export a single position's data."""
    encoding = encode_state(state)
    canonical = canonicalize(encoding)
    moves = generate_moves(state)

    return {
        "canonical": canonical,
        "encoding": encoding,
        "current_player": state.current_player.value,
        "legal_moves": [encode_move(m) for m in moves],
        "legal_move_count": len(moves),
        "winner": get_winner(state),
        "depth": depth,
        "description": description,
    }


def generate_game_tree_positions(max_depth: int = 5, max_positions: int = 50000) -> list[dict]:
    """
    BFS through game tree, collecting positions.

    Returns list of position data dicts.
    """
    positions = []
    visited = set()

    initial = GameState()
    initial_canonical = canonicalize(encode_state(initial))

    # Queue: (state, depth)
    queue = deque([(initial, 0)])
    visited.add(initial_canonical)

    while queue and len(positions) < max_positions:
        state, depth = queue.popleft()

        # Export this position
        positions.append(export_position(state, depth, f"game_tree_depth_{depth}"))

        if depth >= max_depth:
            continue

        # Generate children
        for move in generate_moves(state):
            child_state, result = play_move(state, move)
            child_canonical = canonicalize(encode_state(child_state))

            if child_canonical not in visited:
                visited.add(child_canonical)
                # Only continue exploring if game is ongoing
                if result == GameResult.ONGOING:
                    queue.append((child_state, depth + 1))
                else:
                    # Terminal position - export but don't explore further
                    winner = 1 if result == GameResult.PLAYER_ONE_WINS else 2
                    positions.append({
                        "canonical": child_canonical,
                        "encoding": encode_state(child_state),
                        "current_player": child_state.current_player.value,
                        "legal_moves": [],
                        "legal_move_count": 0,
                        "winner": winner,
                        "depth": depth + 1,
                        "description": f"terminal_depth_{depth+1}",
                    })

    return positions


def generate_edge_case_positions() -> list[dict]:
    """
    Generate specific edge case positions for testing.

    These are hand-crafted to test tricky rules.
    All positions use place_from_reserve() to maintain valid reserve counts.
    """
    positions = []

    # --- Edge Case 1: Basic reveal situation ---
    # P2 has row 0 almost complete, P1 Large covers (0,2)
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))
    state.current_player = Player.ONE
    positions.append(export_position(state, description="reveal_basic_p1_large_covers_p2_row"))

    # --- Edge Case 2: Reveal - piece too small to save ---
    # P2 has row 0 with LARGE pieces, P1 Medium covers one but can't gobble the Larges
    # If P1 lifts the Medium, P2 wins. P1's Medium can only gobble Smalls, not Larges.
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (0, 2))  # Medium covers Small (valid gobble)
    state.current_player = Player.ONE
    # P1's Medium can gobble (0,2) back, but can't gobble the Larges at (0,0) or (0,1)
    positions.append(export_position(state, description="reveal_piece_cannot_gobble_into_line"))

    # --- Edge Case 3: Same-square restriction ---
    # Lifting from (0,2) reveals P2 win, but (0,2) is in winning line
    # P1 cannot place back on same square even though it's "valid"
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))
    state.current_player = Player.ONE
    positions.append(export_position(state, description="reveal_same_square_restriction"))

    # --- Edge Case 4: Reveal but can still win by blocking ---
    # P2 row 0, P1 can gobble and block
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))
    state.current_player = Player.ONE
    # P1 can move Large to (0,0) or (0,1) to block
    positions.append(export_position(state, description="reveal_can_block_by_gobbling"))

    # --- Edge Case 5: Would create own win but reveals opponent win ---
    # P2 row 0 under P1 Large at (0,2), P1 has (1,1) and (2,0)
    # Moving to (2,2) would complete P1 diagonal but reveals P2 row first
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (1, 1))
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (2, 0))
    state.current_player = Player.ONE
    positions.append(export_position(state, description="reveal_would_win_but_reveals_opponent"))

    # --- Edge Case 6: Zugzwang from reveal ---
    # P1 has exhausted all reserves. Only visible piece is a Small that,
    # if lifted, reveals P2's winning line. Small can't gobble into the line.
    # This requires placing all 6 of P1's pieces on the board.
    state = GameState()
    # P2's winning line (row 0) with P1 Small on top of (0,2)
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (0, 2))  # Covers P2's winning line
    # Place P1's remaining pieces, all gobbled by P2 so P1 has no board moves
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (1, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (1, 0))  # Gobbles P1 Small
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (1, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (1, 1))  # Gobbles P1 Small
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (1, 2))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (1, 2))  # Can't gobble Medium, but P1 can't move it without revealing
    # Remaining P1 pieces: 2 Large - place them gobbled too
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (2, 0))
    # P2 has no piece big enough to gobble Large, so P1 can move this one
    # Actually this doesn't create true zugzwang. Let's try a different approach.
    # The key insight: P1's ONLY visible piece must be one that reveals P2 win when moved.
    state = GameState()
    # P2's winning line setup
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))  # P1 Large covers
    # All other P1 pieces gobbled
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (1, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (1, 0))
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (1, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (1, 1))  # Can't gobble Small with Small!
    # Simpler: just use fewer pieces for a cleaner zugzwang
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))  # Only visible P1 piece
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (1, 0))  # Second visible piece
    # P1 has 2 visible pieces. Moving (0,2) Large reveals P2 win, must gobble into row 0.
    # But P2 has Large at (0,0) and (0,1) - P1 Large can't gobble those.
    # P1 CAN move (1,0) Large freely though, so not zugzwang.
    # True zugzwang is hard to construct legally. Skip this edge case.
    # Just test a position where reveal restricts moves significantly.
    state = GameState()
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.LARGE), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 2))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 2))
    state.current_player = Player.ONE
    # P1 can move Large but only to gobble into row 0. Can't gobble (0,0) or (0,1) Larges.
    # Only valid destination is... nowhere! This IS zugzwang for that piece.
    # But P1 still has reserves, so not true zugzwang.
    positions.append(export_position(state, description="reveal_no_valid_destinations_for_piece"))

    # --- Edge Case 7: All pieces stacked on one cell ---
    state = GameState()
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.MEDIUM), (0, 0))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 0))
    state.current_player = Player.TWO
    positions.append(export_position(state, description="stack_three_pieces_one_cell"))

    # --- Edge Case 8: Win by gobbling (completing line) ---
    state = GameState()
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 1))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (0, 2))
    state.current_player = Player.ONE
    # P1 can place Medium on (0,2) to win by gobbling
    positions.append(export_position(state, description="win_by_gobbling"))

    # --- Edge Case 9: Board move (not reserve placement) ---
    state = GameState()
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.TWO, Size.SMALL), (2, 2))
    state.current_player = Player.ONE
    positions.append(export_position(state, description="board_move_available"))

    # --- Edge Case 10: Self-gobbling ---
    state = GameState()
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (1, 1))
    state.current_player = Player.ONE
    # P1 can place Medium or Large on their own Small
    positions.append(export_position(state, description="self_gobbling_available"))

    # --- Edge Case 11: Reserve exhaustion ---
    state = GameState()
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (0, 0))
    place_from_reserve(state, Piece(Player.ONE, Size.SMALL), (0, 1))
    state.current_player = Player.ONE
    # P1 has no more Small pieces in reserve (correctly tracked now)
    positions.append(export_position(state, description="reserve_exhausted_one_size"))

    # --- Edge Case 12: Multiple winning lines ---
    state = GameState()
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (0, 0))
    place_from_reserve(state, Piece(Player.ONE, Size.LARGE), (1, 1))
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (0, 1))
    place_from_reserve(state, Piece(Player.ONE, Size.MEDIUM), (1, 0))
    state.current_player = Player.ONE
    # P1 can win via (2,2) for diagonal or (0,2) for row or (2,0) for column
    positions.append(export_position(state, description="multiple_winning_moves"))

    return positions


def export_all(output_path: Path, max_tree_depth: int = 5, max_tree_positions: int = 50000) -> dict:
    """Export all test positions to JSON file."""
    print(f"Generating game tree positions (depth {max_tree_depth}, max {max_tree_positions})...")
    tree_positions = generate_game_tree_positions(max_tree_depth, max_tree_positions)
    print(f"  Generated {len(tree_positions)} game tree positions")

    print("Generating edge case positions...")
    edge_positions = generate_edge_case_positions()
    print(f"  Generated {len(edge_positions)} edge case positions")

    # Combine and deduplicate by canonical
    all_positions = {}
    for p in tree_positions + edge_positions:
        canonical = p["canonical"]
        if canonical not in all_positions:
            all_positions[canonical] = p
        else:
            # Keep the one with better description
            if p["description"] and not p["description"].startswith("game_tree"):
                all_positions[canonical] = p

    result = {
        "version": "v1",
        "timestamp": datetime.now().isoformat(),
        "stats": {
            "total_positions": len(all_positions),
            "game_tree_positions": len(tree_positions),
            "edge_case_positions": len(edge_positions),
            "max_depth": max_tree_depth,
        },
        "positions": list(all_positions.values()),
    }

    print(f"Writing {len(all_positions)} unique positions to {output_path}...")
    with open(output_path, "w") as f:
        json.dump(result, f, indent=2)

    print("Done!")
    return result


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Export V1 test positions for V2 parity testing")
    parser.add_argument(
        "--output", "-o",
        type=str,
        default="solver/v1_test_positions.json",
        help="Output JSON file path"
    )
    parser.add_argument(
        "--max-depth", "-d",
        type=int,
        default=5,
        help="Maximum BFS depth for game tree (default: 5)"
    )
    parser.add_argument(
        "--max-positions", "-n",
        type=int,
        default=50000,
        help="Maximum positions to export (default: 50000)"
    )

    args = parser.parse_args()

    export_all(
        Path(args.output),
        max_tree_depth=args.max_depth,
        max_tree_positions=args.max_positions,
    )


if __name__ == "__main__":
    main()
