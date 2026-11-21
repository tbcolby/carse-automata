//! Carse Automata rules and data, with no Xous dependencies.
//!
//! * [`grid`]: 64x64 toroidal grid, neighbours, energy.
//! * [`rules`]: the hidden, seed-derived transition rules and death modes.
//! * [`sim`]: one epoch (double-buffered stepping, death detection, new epochs).
//! * [`seed`]: touch-biased seeding, touch map, touching, drift. Entropy is
//!   fail-closed (`Entropy`), and one TRNG draw per call seeds a local generator.
//! * [`codec`]: versioned, validated binary records (and the 0.1 legacy formats).
//! * [`input`]: space-bar hold detection and time-based simulation pacing.
//! * [`draw`]: cell geometry (inclusive pixel offsets).

pub mod codec;
pub mod draw;
pub mod grid;
pub mod input;
pub mod rules;
pub mod seed;
pub mod sim;

pub use grid::{Grid, CELL_COUNT, GRID_SIZE};
pub use rules::{DeathReason, Rules};
pub use sim::Sim;

/// Rule seed of the very first epoch on a fresh install. For bunnie.
pub const FIRST_SEED: u32 = 0xBEEF;
