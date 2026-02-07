pub const GRID_SIZE: usize = 64;
pub const CELL_COUNT: usize = GRID_SIZE * GRID_SIZE;

#[derive(Clone)]
pub struct Grid {
    pub cells: [u8; CELL_COUNT],
}

impl Grid {
    pub fn new() -> Self {
        Grid { cells: [0u8; CELL_COUNT] }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.cells[y * GRID_SIZE + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, val: u8) {
        self.cells[y * GRID_SIZE + x] = val;
    }

    /// Get the 8 Moore neighbors with toroidal wrapping.
    pub fn neighbors(&self, x: usize, y: usize) -> [u8; 8] {
        let n = GRID_SIZE;
        let xm = if x == 0 { n - 1 } else { x - 1 };
        let xp = if x == n - 1 { 0 } else { x + 1 };
        let ym = if y == 0 { n - 1 } else { y - 1 };
        let yp = if y == n - 1 { 0 } else { y + 1 };
        [
            self.get(xm, ym), self.get(x, ym), self.get(xp, ym),
            self.get(xm, y),                   self.get(xp, y),
            self.get(xm, yp), self.get(x, yp), self.get(xp, yp),
        ]
    }

    /// Sum of all cell states (total energy).
    pub fn total_energy(&self) -> u32 {
        self.cells.iter().map(|&c| c as u32).sum()
    }

    /// Check equality with another grid (for stagnation detection).
    pub fn equals(&self, other: &Grid) -> bool {
        self.cells == other.cells
    }

    /// Seed the grid randomly, biased by the touch map.
    /// Higher touch counts → higher probability of starting alive and in higher energy states.
    pub fn seed_random(&mut self, trng: &trng::Trng, touch_map: &[u32; CELL_COUNT]) {
        let max_touch = touch_map.iter().copied().max().unwrap_or(1).max(1);

        for i in 0..CELL_COUNT {
            let r = trng.get_u32().unwrap_or(0);
            let touch_weight = touch_map[i] as u64 * 100 / max_touch as u64; // 0..100

            // Base 15% chance of being non-void, boosted up to 50% by touch
            let threshold = 15 + (touch_weight as u32 * 35 / 100); // 15..50
            let roll = r % 100;

            if roll < threshold {
                // Alive: pick state biased by touch weight
                // Higher touch → higher average starting state
                let state_roll = (r >> 8) % 100;
                let bias = touch_weight as u32;
                let state = if state_roll < 10 + bias / 2 {
                    // High energy
                    ((r >> 16) % 3 + 4).min(6) as u8 // 4, 5, or 6
                } else if state_roll < 40 + bias / 3 {
                    // Mid energy
                    ((r >> 20) % 2 + 2) as u8 // 2 or 3
                } else {
                    // Low energy
                    1u8
                };
                self.cells[i] = state;
            } else {
                self.cells[i] = 0;
            }
        }
    }

    /// Seed with a specific pattern at a position (for easter eggs).
    pub fn place_bridge_glider(&mut self, cx: usize, cy: usize) {
        // A bridge-shaped glider pattern
        let pattern: [(isize, isize, u8); 12] = [
            (-2, -1, 3), (-1, -1, 4), (0, -1, 3), (1, -1, 4), (2, -1, 3),
            (-1,  0, 5), (1,  0, 5),
            (-2,  1, 3), (-1,  1, 4), (0,  1, 5), (1,  1, 4), (2,  1, 3),
        ];
        for &(dx, dy, state) in &pattern {
            let x = ((cx as isize + dx).rem_euclid(GRID_SIZE as isize)) as usize;
            let y = ((cy as isize + dy).rem_euclid(GRID_SIZE as isize)) as usize;
            self.set(x, y, state);
        }
    }
}
