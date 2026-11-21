//! Seeding, touching and drift. All randomness is fail-closed: a failed draw
//! returns `EntropyError` and nothing is changed.
//!
//! Seeding a grid needs thousands of random decisions. 0.1 made one blocking
//! TRNG IPC per cell and hid TRNG failure with `unwrap_or(0)`. Now four TRNG
//! words seed a local [`LocalRng`] (xoshiro128**), and every decision uses the
//! unbiased [`entropy::below`]. A failed TRNG draw aborts before the grid is touched.

use precursor_appkit_core::entropy::{self, Entropy, EntropyError};

use crate::grid::{Grid, CELL_COUNT, MAX_STATE};

/// xoshiro128** seeded from real entropy; never fails once constructed.
pub struct LocalRng {
    s: [u32; 4],
}

impl LocalRng {
    /// Draw four words from `e`. Fails if the source fails or is all zero.
    pub fn from_entropy<E: Entropy + ?Sized>(e: &mut E) -> Result<Self, EntropyError> {
        let s = [e.try_u32()?, e.try_u32()?, e.try_u32()?, e.try_u32()?];
        if s == [0; 4] {
            return Err(EntropyError);
        }
        Ok(Self { s })
    }
}

impl Entropy for LocalRng {
    fn try_u32(&mut self) -> Result<u32, EntropyError> {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 9;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(11);
        Ok(result)
    }
}

/// How many times each cell has been touched, across all epochs, forever.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TouchMap {
    counts: Vec<u32>,
}

impl Default for TouchMap {
    fn default() -> Self { Self::new() }
}

impl TouchMap {
    pub fn new() -> Self { Self { counts: vec![0; CELL_COUNT] } }

    pub fn from_vec(counts: Vec<u32>) -> Option<Self> {
        (counts.len() == CELL_COUNT).then_some(Self { counts })
    }

    pub fn counts(&self) -> &[u32] { &self.counts }

    pub fn get(&self, idx: usize) -> u32 { self.counts[idx] }

    pub fn max(&self) -> u32 { self.counts.iter().copied().max().unwrap_or(0) }
}

/// Seed the grid randomly, biased by the touch map: higher touch counts start
/// alive more often and at higher energy. On error the grid is unchanged.
pub fn seed_random<E: Entropy + ?Sized>(grid: &mut Grid, touch: &TouchMap, e: &mut E) -> Result<(), EntropyError> {
    let mut rng = LocalRng::from_entropy(e)?;
    let max_touch = touch.max().max(1) as u64;
    for i in 0..CELL_COUNT {
        let weight = (touch.get(i) as u64 * 100 / max_touch) as u32; // 0..=100
        let threshold = 15 + weight * 35 / 100; // 15..=50
        let alive = entropy::below(&mut rng, 100)? < threshold;
        grid.cells[i] = if alive {
            let state_roll = entropy::below(&mut rng, 100)?;
            if state_roll < 10 + weight / 2 {
                4 + entropy::below(&mut rng, 3)? as u8
            } else if state_roll < 40 + weight / 3 {
                2 + entropy::below(&mut rng, 2)? as u8
            } else {
                1
            }
        } else {
            0
        };
    }
    Ok(())
}

/// Drift applies if more than a day of wall-clock time passed between sessions.
pub const DRIFT_AFTER_MS: u64 = 86_400_000;

/// `last` and `now` are wall-clock milliseconds (`llio` local time), not uptime:
/// uptime resets at boot, so 0.1's comparison could essentially never be true.
/// Unknown on either side (clock not set) means no drift.
pub fn drift_due(last: Option<u64>, now: Option<u64>) -> bool {
    match (last, now) {
        (Some(l), Some(n)) => n > l && n - l > DRIFT_AFTER_MS,
        _ => false,
    }
}

/// Drift: about 5% of living cells move by one.
pub fn apply_drift<E: Entropy + ?Sized>(grid: &mut Grid, e: &mut E) -> Result<(), EntropyError> {
    let mut rng = LocalRng::from_entropy(e)?;
    let mut next = grid.clone();
    for i in 0..CELL_COUNT {
        if next.cells[i] > 0 && entropy::below(&mut rng, 20)? == 0 {
            next.cells[i] = if entropy::below(&mut rng, 2)? == 0 {
                (next.cells[i] + 1).min(MAX_STATE)
            } else {
                next.cells[i] - 1
            };
        }
    }
    *grid = next;
    Ok(())
}

/// What a touch changed, so a touch that turns out to be the start of a hold can be undone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touch {
    pub idx: usize,
    prev_state: u8,
    prev_count: u32,
}

/// Perturb the cell at `idx` and count the touch. Needs entropy only for Warm/Hot cells.
pub fn touch_cell<E: Entropy + ?Sized>(
    grid: &mut Grid,
    touch: &mut TouchMap,
    idx: usize,
    e: &mut E,
) -> Result<Touch, EntropyError> {
    let state = grid.cells[idx];
    let new_state = match state {
        0 => 1,
        1..=3 => state + 1,
        4 | 5 => match entropy::below(e, 3)? {
            0 => (state + 1).min(MAX_STATE),
            1 => state,
            _ => state - 1,
        },
        _ => 3, // Burning -> Neutral
    };
    let t = Touch { idx, prev_state: state, prev_count: touch.counts[idx] };
    touch.counts[idx] = touch.counts[idx].saturating_add(1);
    grid.cells[idx] = new_state;
    Ok(t)
}

/// Revert a [`touch_cell`].
pub fn undo_touch(grid: &mut Grid, touch: &mut TouchMap, t: Touch) {
    grid.cells[t.idx] = t.prev_state;
    touch.counts[t.idx] = t.prev_count;
}

#[cfg(test)]
mod tests {
    use super::*;
    use precursor_appkit_core::entropy::testing::*;

    #[test]
    fn seeding_fails_closed_and_leaves_grid_alone() {
        let mut g = Grid::new();
        g.cells[3] = 5;
        assert!(seed_random(&mut g, &TouchMap::new(), &mut Broken).is_err());
        assert_eq!(g.cells[3], 5);
        assert!(seed_random(&mut g, &TouchMap::new(), &mut Seq(vec![1, 2, 3], 0)).is_err(), "runs dry");
        assert_eq!(g.cells[3], 5);
        assert!(LocalRng::from_entropy(&mut Stuck(0)).is_err(), "all-zero state rejected");
    }

    #[test]
    fn seeding_density_is_about_fifteen_percent_and_states_valid() {
        let mut g = Grid::new();
        seed_random(&mut g, &TouchMap::new(), &mut XorShift(0xC0FFEE)).unwrap();
        assert!(g.cells.iter().all(|&c| c <= 6));
        let alive = g.cells.iter().filter(|&&c| c > 0).count();
        // 15% of 4096 = 614; allow wide margin
        assert!((450..800).contains(&alive), "alive = {alive}");
        // a different TRNG stream gives a different grid
        let mut g2 = Grid::new();
        seed_random(&mut g2, &TouchMap::new(), &mut XorShift(0xBADF00D)).unwrap();
        assert!(g.cells != g2.cells);
    }

    #[test]
    fn touched_cells_start_alive_more_often() {
        let mut v = vec![0u32; CELL_COUNT];
        for c in v.iter_mut().take(CELL_COUNT / 2) {
            *c = 100;
        }
        let touch = TouchMap::from_vec(v).unwrap();
        let mut g = Grid::new();
        seed_random(&mut g, &touch, &mut XorShift(7)).unwrap();
        let touched_alive = g.cells[..CELL_COUNT / 2].iter().filter(|&&c| c > 0).count();
        let untouched_alive = g.cells[CELL_COUNT / 2..].iter().filter(|&&c| c > 0).count();
        assert!(touched_alive > untouched_alive * 2, "{touched_alive} vs {untouched_alive}");
    }

    #[test]
    fn drift_needs_wall_clock_gap_over_a_day() {
        assert!(!drift_due(None, Some(10)));
        assert!(!drift_due(Some(10), None));
        assert!(!drift_due(Some(1_000), Some(1_000 + DRIFT_AFTER_MS)));
        assert!(drift_due(Some(1_000), Some(1_001 + DRIFT_AFTER_MS)));
        assert!(!drift_due(Some(5_000_000_000), Some(1_000)), "clock set backwards");
    }

    #[test]
    fn drift_changes_few_cells_by_one_and_fails_closed() {
        let mut g = Grid::new();
        for c in g.cells.iter_mut() {
            *c = 3;
        }
        let before = g.clone();
        apply_drift(&mut g, &mut XorShift(99)).unwrap();
        let changed = g.cells.iter().zip(before.cells.iter()).filter(|(a, b)| a != b).count();
        assert!((100..350).contains(&changed), "changed = {changed}"); // ~5% of 4096 = 205
        assert!(g.cells.iter().zip(before.cells.iter()).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 1));
        let mut g2 = before.clone();
        assert!(apply_drift(&mut g2, &mut Broken).is_err());
        assert!(g2 == before);
        // void cells never change
        let mut v = Grid::new();
        apply_drift(&mut v, &mut XorShift(1)).unwrap();
        assert_eq!(v.total_energy(), 0);
    }

    #[test]
    fn touch_perturbs_counts_and_undoes() {
        let mut g = Grid::new();
        let mut t = TouchMap::new();
        let mut e = XorShift(5);
        let rec = touch_cell(&mut g, &mut t, 10, &mut e).unwrap();
        assert_eq!(g.cells[10], 1);
        assert_eq!(t.get(10), 1);
        undo_touch(&mut g, &mut t, rec);
        assert_eq!(g.cells[10], 0);
        assert_eq!(t.get(10), 0);
        // each state
        for (from, to) in [(1u8, 2u8), (2, 3), (3, 4), (6, 3)] {
            g.cells[20] = from;
            touch_cell(&mut g, &mut t, 20, &mut e).unwrap();
            assert_eq!(g.cells[20], to);
        }
        for _ in 0..50 {
            g.cells[30] = 4;
            touch_cell(&mut g, &mut t, 30, &mut e).unwrap();
            assert!((3..=5).contains(&g.cells[30]));
            g.cells[30] = 5;
            touch_cell(&mut g, &mut t, 30, &mut e).unwrap();
            assert!((4..=6).contains(&g.cells[30]));
        }
    }

    #[test]
    fn failed_touch_changes_nothing() {
        let mut g = Grid::new();
        g.cells[7] = 4;
        let mut t = TouchMap::new();
        assert!(touch_cell(&mut g, &mut t, 7, &mut Broken).is_err());
        assert_eq!(g.cells[7], 4);
        assert_eq!(t.get(7), 0);
    }

    #[test]
    fn touch_count_saturates() {
        let mut v = vec![0u32; CELL_COUNT];
        v[1] = u32::MAX;
        let mut t = TouchMap::from_vec(v).unwrap();
        let mut g = Grid::new();
        touch_cell(&mut g, &mut t, 1, &mut XorShift(1)).unwrap();
        assert_eq!(t.get(1), u32::MAX);
        assert!(TouchMap::from_vec(vec![0; 3]).is_none());
    }
}
