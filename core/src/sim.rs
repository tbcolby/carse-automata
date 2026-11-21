//! One epoch of the simulation: double-buffered stepping and death detection.

use precursor_appkit_core::entropy::{Entropy, EntropyError};

use crate::grid::{Grid, GRID_SIZE};
use crate::rules::{DeathReason, Rules};
use crate::seed::{seed_random, TouchMap};
use crate::FIRST_SEED;

/// Epoch state. Grids are boxed so moving a `Sim` doesn't copy 12 KB.
pub struct Sim {
    pub grid: Box<Grid>,
    prev: Box<Grid>,
    prev2: Box<Grid>,
    pub rules: Rules,
    pub epoch: u32,
    pub tick: u32,
    pub death: Option<DeathReason>,
}

impl Sim {
    /// A fresh simulation of epoch 0 with the first seed and a seeded grid.
    pub fn fresh<E: Entropy + ?Sized>(touch: &TouchMap, e: &mut E) -> Result<Self, EntropyError> {
        let mut grid = Box::<Grid>::default();
        seed_random(&mut grid, touch, e)?;
        Ok(Self::from_parts(grid, 0, 0, FIRST_SEED))
    }

    /// Restore a saved epoch.
    pub fn from_parts(grid: Box<Grid>, epoch: u32, tick: u32, seed: u32) -> Self {
        Self {
            prev: grid.clone(),
            prev2: grid.clone(),
            grid,
            rules: Rules::from_seed(seed),
            epoch,
            tick,
            death: None,
        }
    }

    pub fn is_dead(&self) -> bool { self.death.is_some() }

    /// Advance one generation (no-op once dead). The three buffers rotate; nothing is cloned.
    pub fn advance(&mut self) {
        if self.is_dead() {
            return;
        }
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let n = self.grid.neighbors(x, y);
                let next = self.rules.transition(self.grid.get(x, y), &n);
                self.prev2.set(x, y, next);
            }
        }
        // prev2 holds the new generation; rotate so grid=new, prev=old grid, prev2=old prev.
        std::mem::swap(&mut self.prev2, &mut self.prev);
        std::mem::swap(&mut self.prev, &mut self.grid);
        self.tick += 1;
        self.check_death();
    }

    fn check_death(&mut self) {
        let energy = self.grid.total_energy();
        self.death = if energy == 0 {
            Some(DeathReason::HeatDeath)
        } else if energy > self.rules.collapse_energy {
            Some(DeathReason::Collapse)
        } else if *self.grid == *self.prev || *self.grid == *self.prev2 {
            Some(DeathReason::Stagnation)
        } else {
            None
        };
    }

    /// Begin the next epoch: mutate the seed, reseed the grid from the touch map.
    /// On entropy failure the current (dead) epoch is left as it was.
    pub fn next_epoch<E: Entropy + ?Sized>(&mut self, touch: &TouchMap, e: &mut E) -> Result<(), EntropyError> {
        let mut grid = Box::<Grid>::default();
        seed_random(&mut grid, touch, e)?;
        // Easter egg: cell (7,7) touched exactly 49 times places a bridge glider.
        if touch.get(7 * GRID_SIZE + 7) == 49 {
            grid.place_bridge_glider(32, 32);
        }
        self.rules = Rules::from_seed(Rules::next_seed(self.rules.seed));
        self.epoch += 1;
        self.tick = 0;
        self.death = None;
        self.prev = Box::<Grid>::default();
        self.prev2 = Box::<Grid>::default();
        self.grid = grid;
        Ok(())
    }

    /// Reset the death-detection history (after the grid was edited by hand).
    pub fn forget_history(&mut self) {
        *self.prev = Grid::new();
        *self.prev2 = Grid::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use precursor_appkit_core::entropy::testing::*;

    fn sim_with(cells: &[(usize, usize, u8)]) -> Sim {
        let mut g = Box::<Grid>::default();
        for &(x, y, s) in cells {
            g.set(x, y, s);
        }
        Sim::from_parts(g, 0, 0, FIRST_SEED)
    }

    #[test]
    fn empty_grid_is_heat_death_after_one_step() {
        let mut s = sim_with(&[]);
        s.advance();
        assert_eq!(s.death, Some(DeathReason::HeatDeath));
        let t = s.tick;
        s.advance();
        assert_eq!(s.tick, t, "dead epochs don't advance");
    }

    #[test]
    fn isolated_cell_decays_to_heat_death() {
        let mut s = sim_with(&[(10, 10, 3)]);
        s.advance();
        assert_eq!(s.grid.get(10, 10), 2);
        assert_eq!(s.death, None);
        s.advance();
        s.advance();
        assert_eq!(s.grid.get(10, 10), 0);
        assert_eq!(s.death, Some(DeathReason::HeatDeath));
        assert_eq!(s.tick, 3);
    }

    #[test]
    fn buffers_rotate_correctly() {
        let mut s = sim_with(&[(10, 10, 3)]);
        s.advance(); // grid: 2, prev: 3
        assert_eq!((s.grid.get(10, 10), s.prev.get(10, 10)), (2, 3));
        s.advance(); // grid: 1, prev: 2, prev2: 3
        assert_eq!((s.grid.get(10, 10), s.prev.get(10, 10), s.prev2.get(10, 10)), (1, 2, 3));
    }

    #[test]
    fn stagnation_detected_for_static_and_period_two() {
        // A full grid of Burning stays Burning (sustained inferno) = static... but exceeds collapse
        // energy first, so build a still life by hand with a tiny rule-free check instead:
        let mut s = sim_with(&[(5, 5, 3)]);
        // force prev == grid to model a still life
        *s.prev = (*s.grid).clone();
        s.check_death();
        assert_eq!(s.death, Some(DeathReason::Stagnation));
        let mut s = sim_with(&[(5, 5, 3)]);
        *s.prev2 = (*s.grid).clone();
        s.check_death();
        assert_eq!(s.death, Some(DeathReason::Stagnation), "period 2");
    }

    #[test]
    fn collapse_when_energy_exceeds_threshold() {
        let mut g = Box::<Grid>::default();
        for c in g.cells.iter_mut() {
            *c = 6;
        }
        let mut s = Sim::from_parts(g, 0, 0, FIRST_SEED);
        assert!(s.grid.total_energy() > s.rules.collapse_energy, "full grid exceeds any threshold");
        s.advance();
        assert_eq!(s.death, Some(DeathReason::Collapse));
    }

    #[test]
    fn next_epoch_resets_and_advances_seed() {
        let touch = TouchMap::new();
        let mut s = Sim::fresh(&touch, &mut XorShift(42)).unwrap();
        s.advance();
        let old_seed = s.rules.seed;
        s.next_epoch(&touch, &mut XorShift(43)).unwrap();
        assert_eq!((s.epoch, s.tick, s.death), (1, 0, None));
        assert_eq!(s.rules.seed, Rules::next_seed(old_seed));
        assert!(s.grid.total_energy() > 0);
    }

    #[test]
    fn next_epoch_failure_changes_nothing() {
        let touch = TouchMap::new();
        let mut s = sim_with(&[]);
        s.advance();
        assert!(s.next_epoch(&touch, &mut Broken).is_err());
        assert_eq!((s.epoch, s.death), (0, Some(DeathReason::HeatDeath)));
        assert!(Sim::fresh(&touch, &mut Broken).is_err());
    }

    #[test]
    fn seventh_cell_easter_egg_places_glider() {
        let mut v = vec![0u32; crate::CELL_COUNT];
        v[7 * GRID_SIZE + 7] = 49;
        let touch = TouchMap::from_vec(v).unwrap();
        let mut s = sim_with(&[]);
        s.next_epoch(&touch, &mut XorShift(1)).unwrap();
        assert_eq!(s.grid.get(33, 32), 5);
        assert_eq!(s.grid.get(31, 32), 5);
    }
}
