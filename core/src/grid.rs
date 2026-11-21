//! The 64x64 toroidal grid.

pub const GRID_SIZE: usize = 64;
pub const CELL_COUNT: usize = GRID_SIZE * GRID_SIZE;
/// Highest cell state (Burning).
pub const MAX_STATE: u8 = 6;

#[derive(Clone, PartialEq, Eq)]
pub struct Grid {
    pub cells: [u8; CELL_COUNT],
}

impl std::fmt::Debug for Grid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Grid(energy {})", self.total_energy())
    }
}

impl Default for Grid {
    fn default() -> Self { Self::new() }
}

impl Grid {
    pub fn new() -> Self { Grid { cells: [0u8; CELL_COUNT] } }

    /// Build from raw bytes; `None` unless there are exactly `CELL_COUNT` and all are valid states.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != CELL_COUNT || bytes.iter().any(|&c| c > MAX_STATE) {
            return None;
        }
        let mut g = Grid::new();
        g.cells.copy_from_slice(bytes);
        Some(g)
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u8 { self.cells[y * GRID_SIZE + x] }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, val: u8) { self.cells[y * GRID_SIZE + x] = val; }

    /// The 8 Moore neighbours with toroidal wrapping.
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

    /// Sum of all cell states.
    pub fn total_energy(&self) -> u32 { self.cells.iter().map(|&c| c as u32).sum() }

    /// Place a bridge-shaped pattern centred on `(cx, cy)` (an easter egg).
    pub fn place_bridge_glider(&mut self, cx: usize, cy: usize) {
        const PATTERN: [(isize, isize, u8); 12] = [
            (-2, -1, 3), (-1, -1, 4), (0, -1, 3), (1, -1, 4), (2, -1, 3),
            (-1, 0, 5), (1, 0, 5),
            (-2, 1, 3), (-1, 1, 4), (0, 1, 5), (1, 1, 4), (2, 1, 3),
        ];
        for &(dx, dy, state) in &PATTERN {
            let x = (cx as isize + dx).rem_euclid(GRID_SIZE as isize) as usize;
            let y = (cy as isize + dy).rem_euclid(GRID_SIZE as isize) as usize;
            self.set(x, y, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_wrap_at_every_corner() {
        let mut g = Grid::new();
        g.set(63, 63, 1);
        g.set(0, 0, 2);
        g.set(63, 0, 3);
        g.set(0, 63, 4);
        // from (0,0) the diagonal up-left is (63,63); others wrap too
        let n = g.neighbors(0, 0);
        assert_eq!(n[0], 1, "up-left");
        assert_eq!(n[1], 4, "up");
        assert_eq!(n[3], 3, "left");
        assert_eq!(n.iter().map(|&v| v as u32).sum::<u32>(), 1 + 4 + 3);
        let n = g.neighbors(63, 63);
        assert_eq!(n[7], 2, "down-right wraps to (0,0)");
    }

    #[test]
    fn neighbours_exclude_self_and_count_eight() {
        let mut g = Grid::new();
        g.set(10, 10, 6);
        assert_eq!(g.neighbors(10, 10), [0; 8]);
        assert_eq!(g.neighbors(11, 10).iter().filter(|&&v| v == 6).count(), 1);
    }

    #[test]
    fn from_bytes_validates() {
        assert!(Grid::from_bytes(&[0; CELL_COUNT]).is_some());
        assert!(Grid::from_bytes(&[0; 10]).is_none());
        let mut b = [0u8; CELL_COUNT];
        b[5] = 7;
        assert!(Grid::from_bytes(&b).is_none(), "state above 6 rejected");
    }

    #[test]
    fn glider_wraps_near_edge() {
        let mut g = Grid::new();
        g.place_bridge_glider(0, 0);
        assert!(g.total_energy() > 0);
        assert_eq!(g.get(62, 63), 3);
    }
}
