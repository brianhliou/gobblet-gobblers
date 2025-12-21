"""Tests for solver checkpoint functionality."""

from pathlib import Path
import tempfile

import pytest

from solver.checkpoint import (
    clear_checkpoint,
    get_checkpoint_stats,
    init_db,
    load_checkpoint,
    save_checkpoint,
)
from solver.minimax import Outcome, Solver


@pytest.fixture
def temp_db():
    """Create a temporary database path for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir) / "test_solver.db"


class TestInitDb:
    def test_creates_tables(self, temp_db):
        conn = init_db(temp_db)
        cursor = conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        )
        tables = {row[0] for row in cursor}
        conn.close()

        assert "transposition" in tables
        assert "metadata" in tables

    def test_idempotent(self, temp_db):
        """Can call init_db multiple times safely."""
        init_db(temp_db).close()
        init_db(temp_db).close()
        assert temp_db.exists()


class TestSaveAndLoad:
    def test_save_and_load_roundtrip(self, temp_db):
        # Create solver with some data
        solver = Solver()
        solver.table[12345] = Outcome.WIN_P1
        solver.table[67890] = Outcome.DRAW
        solver.table[11111] = Outcome.WIN_P2
        solver.stats.positions_evaluated = 100
        solver.stats.cache_hits = 50
        solver.stats.max_depth = 25

        # Save
        saved = save_checkpoint(solver, temp_db)
        assert saved == 3

        # Load into fresh solver
        solver2 = Solver()
        loaded = load_checkpoint(solver2, temp_db)
        assert loaded == 3

        # Verify data
        assert solver2.table[12345] == Outcome.WIN_P1
        assert solver2.table[67890] == Outcome.DRAW
        assert solver2.table[11111] == Outcome.WIN_P2
        assert solver2.stats.positions_evaluated == 100
        assert solver2.stats.cache_hits == 50
        assert solver2.stats.max_depth == 25

    def test_load_nonexistent_returns_zero(self, temp_db):
        solver = Solver()
        loaded = load_checkpoint(solver, temp_db)
        assert loaded == 0
        assert len(solver.table) == 0

    def test_save_updates_existing(self, temp_db):
        solver = Solver()
        solver.table[12345] = Outcome.WIN_P1

        save_checkpoint(solver, temp_db)

        # Update and save again
        solver.table[12345] = Outcome.DRAW
        solver.table[99999] = Outcome.WIN_P2
        save_checkpoint(solver, temp_db)

        # Load and verify
        solver2 = Solver()
        load_checkpoint(solver2, temp_db)
        assert solver2.table[12345] == Outcome.DRAW
        assert solver2.table[99999] == Outcome.WIN_P2


class TestGetCheckpointStats:
    def test_returns_none_if_no_checkpoint(self, temp_db):
        stats = get_checkpoint_stats(temp_db)
        assert stats is None

    def test_returns_stats(self, temp_db):
        solver = Solver()
        solver.table[1] = Outcome.WIN_P1
        solver.table[2] = Outcome.WIN_P2
        solver.stats.positions_evaluated = 42
        save_checkpoint(solver, temp_db)

        stats = get_checkpoint_stats(temp_db)
        assert stats is not None
        assert stats["unique_positions"] == 2
        assert stats["positions_evaluated"] == "42"


class TestClearCheckpoint:
    def test_clears_checkpoint(self, temp_db):
        solver = Solver()
        solver.table[1] = Outcome.WIN_P1
        save_checkpoint(solver, temp_db)
        assert temp_db.exists()

        clear_checkpoint(temp_db)
        assert not temp_db.exists()

    def test_clear_nonexistent_is_noop(self, temp_db):
        # Should not raise
        clear_checkpoint(temp_db)
