//! Gobblet Gobblers solver library.
//!
//! `movegen` is the shared move generator used by the retrograde solver
//! (`src/bin/retro.rs`) and the analysis binaries. The forward minimax solver
//! that previously lived here was retired — see `legacy/forward-solver/`.

pub mod movegen;
