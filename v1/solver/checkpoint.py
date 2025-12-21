"""
Checkpointing support for the Gobblet Gobblers solver.

Provides SQLite-based persistence for the transposition table,
allowing solves to be paused and resumed.
"""

from __future__ import annotations

import sqlite3
import time
from pathlib import Path
from typing import TYPE_CHECKING

from solver.minimax import Outcome

if TYPE_CHECKING:
    from solver.minimax import Solver

DEFAULT_DB_PATH = Path("solver/gobblet_solver.db")


class IncrementalCheckpointer:
    """
    Manages incremental checkpointing to SQLite.

    Tracks which positions have been saved and only writes new ones.
    Supports time-based automatic checkpointing.
    """

    def __init__(self, db_path: Path = DEFAULT_DB_PATH, checkpoint_interval_sec: float = 60.0):
        self.db_path = db_path
        self.checkpoint_interval_sec = checkpoint_interval_sec
        self._saved_positions: set[int] = set()
        self._last_checkpoint_time: float = time.time()
        self._conn: sqlite3.Connection | None = None

    def initialize(self, solver: Solver) -> int:
        """Load existing checkpoint and track saved positions."""
        count = load_checkpoint(solver, self.db_path)
        self._saved_positions = set(solver.table.keys())
        self._last_checkpoint_time = time.time()
        return count

    def maybe_checkpoint(self, solver: Solver) -> int:
        """
        Save checkpoint if enough time has passed.

        Returns number of new positions saved (0 if no checkpoint).
        """
        now = time.time()
        if now - self._last_checkpoint_time < self.checkpoint_interval_sec:
            return 0

        return self.force_checkpoint(solver)

    def force_checkpoint(self, solver: Solver) -> int:
        """
        Save all new positions since last checkpoint.

        Returns number of new positions saved.
        """
        # Find positions not yet saved
        new_positions = {
            canonical: solver.table[canonical]
            for canonical in solver.table
            if canonical not in self._saved_positions
        }

        if not new_positions:
            self._last_checkpoint_time = time.time()
            return 0

        conn = init_db(self.db_path)
        try:
            # Batch insert new positions
            batch = [(k, int(v)) for k, v in new_positions.items()]
            conn.executemany(
                "INSERT OR REPLACE INTO transposition (canonical, outcome) VALUES (?, ?)",
                batch
            )

            # Update metadata
            conn.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
                ("positions_evaluated", str(solver.stats.positions_evaluated))
            )
            conn.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
                ("cache_hits", str(solver.stats.cache_hits))
            )
            conn.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
                ("terminal_positions", str(solver.stats.terminal_positions))
            )
            conn.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
                ("cycle_draws", str(solver.stats.cycle_draws))
            )
            conn.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
                ("max_depth", str(solver.stats.max_depth))
            )

            conn.commit()

            # Update tracking
            self._saved_positions.update(new_positions.keys())
            self._last_checkpoint_time = time.time()

            return len(new_positions)
        finally:
            conn.close()


def init_db(db_path: Path = DEFAULT_DB_PATH) -> sqlite3.Connection:
    """
    Initialize the checkpoint database.

    Creates the schema if it doesn't exist.
    """
    db_path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(db_path))

    conn.execute("""
        CREATE TABLE IF NOT EXISTS transposition (
            canonical INTEGER PRIMARY KEY,
            outcome INTEGER NOT NULL
        )
    """)

    conn.execute("""
        CREATE TABLE IF NOT EXISTS metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
    """)

    conn.commit()
    return conn


def save_checkpoint(
    solver: Solver,
    db_path: Path = DEFAULT_DB_PATH,
    batch_size: int = 10000
) -> int:
    """
    Save solver transposition table to SQLite.

    Uses INSERT OR REPLACE for idempotent saves.
    Returns number of entries saved.
    """
    conn = init_db(db_path)

    try:
        # Batch insert for performance
        entries = list(solver.table.items())
        for i in range(0, len(entries), batch_size):
            batch = entries[i:i + batch_size]
            conn.executemany(
                "INSERT OR REPLACE INTO transposition (canonical, outcome) VALUES (?, ?)",
                [(k, int(v)) for k, v in batch]
            )

        # Save metadata
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
            ("positions_evaluated", str(solver.stats.positions_evaluated))
        )
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
            ("cache_hits", str(solver.stats.cache_hits))
        )
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
            ("terminal_positions", str(solver.stats.terminal_positions))
        )
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
            ("cycle_draws", str(solver.stats.cycle_draws))
        )
        conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)",
            ("max_depth", str(solver.stats.max_depth))
        )

        conn.commit()
        return len(entries)
    finally:
        conn.close()


def load_checkpoint(
    solver: Solver,
    db_path: Path = DEFAULT_DB_PATH
) -> int:
    """
    Load transposition table from SQLite checkpoint.

    Returns number of entries loaded.
    """
    if not db_path.exists():
        return 0

    conn = sqlite3.connect(str(db_path))

    try:
        # Load transposition table
        cursor = conn.execute("SELECT canonical, outcome FROM transposition")
        count = 0
        for canonical, outcome_int in cursor:
            solver.table[canonical] = Outcome(outcome_int)
            count += 1

        # Load metadata
        cursor = conn.execute("SELECT key, value FROM metadata")
        for key, value in cursor:
            if key == "positions_evaluated":
                solver.stats.positions_evaluated = int(value)
            elif key == "cache_hits":
                solver.stats.cache_hits = int(value)
            elif key == "terminal_positions":
                solver.stats.terminal_positions = int(value)
            elif key == "cycle_draws":
                solver.stats.cycle_draws = int(value)
            elif key == "max_depth":
                solver.stats.max_depth = int(value)

        return count
    finally:
        conn.close()


def get_checkpoint_stats(db_path: Path = DEFAULT_DB_PATH) -> dict | None:
    """
    Get statistics about an existing checkpoint.

    Returns None if no checkpoint exists.
    """
    if not db_path.exists():
        return None

    conn = sqlite3.connect(str(db_path))

    try:
        cursor = conn.execute("SELECT COUNT(*) FROM transposition")
        count = cursor.fetchone()[0]

        metadata = {}
        cursor = conn.execute("SELECT key, value FROM metadata")
        for key, value in cursor:
            metadata[key] = value

        return {
            "unique_positions": count,
            **metadata
        }
    finally:
        conn.close()


def clear_checkpoint(db_path: Path = DEFAULT_DB_PATH) -> None:
    """Delete the checkpoint database."""
    if db_path.exists():
        db_path.unlink()
