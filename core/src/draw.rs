//! Cell geometry. A cell is `CELL_PX` pixels square. GAM rectangles are
//! inclusive at the bottom-right, so a cell spans offsets `0..=CELL_PX-1`;
//! 0.1 used `+ CELL_PX` and every shape was a pixel too big, erasing the
//! neighbours' top row and left column.

pub const CELL_PX: i32 = 5;

/// Inclusive pixel rectangle inside a cell: `(x0, y0, x1, y1)`.
pub type CellRect = (i32, i32, i32, i32);

/// The whole cell (used to erase to dark before drawing a pattern).
pub const FULL: CellRect = (0, 0, CELL_PX - 1, CELL_PX - 1);

/// Light rectangle for a state, drawn on the dark cell. `None` for Void.
pub fn light_rect(state: u8) -> Option<CellRect> {
    Some(match state {
        1 => (2, 2, 2, 2), // 1x1
        2 => (1, 1, 2, 2), // 2x2
        3 => (1, 1, 3, 3), // 3x3
        4 => (0, 0, 3, 3), // 4x4
        5 => (0, 0, 4, 3), // 5x4
        6 => FULL,         // 5x5
        _ => return None,
    })
}

/// The four edge rectangles of the cursor frame (inclusive).
pub fn cursor_frame() -> [CellRect; 4] {
    let m = CELL_PX - 1;
    [(0, 0, m, 0), (0, m, m, m), (0, 1, 0, m - 1), (m, 1, m, m - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(r: CellRect) -> i32 { (r.2 - r.0 + 1) * (r.3 - r.1 + 1) }

    #[test]
    fn rects_stay_inside_the_cell_and_grow_with_energy() {
        let mut last = 0;
        for s in 1..=6u8 {
            let r = light_rect(s).unwrap();
            assert!(r.0 >= 0 && r.1 >= 0 && r.2 <= CELL_PX - 1 && r.3 <= CELL_PX - 1, "state {s} {r:?}");
            assert!(area(r) > last);
            last = area(r);
        }
        assert_eq!(area(FULL), 25, "erase rect is 5x5, not 6x6");
        assert_eq!(area(light_rect(1).unwrap()), 1);
        assert_eq!(area(light_rect(6).unwrap()), 25);
        assert!(light_rect(0).is_none());
        assert!(light_rect(7).is_none());
    }

    #[test]
    fn cursor_frame_covers_the_border_exactly_once() {
        let mut hit = [[0u8; 5]; 5];
        for (x0, y0, x1, y1) in cursor_frame() {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    hit[y as usize][x as usize] += 1;
                }
            }
        }
        for (y, row) in hit.iter().enumerate() {
            for (x, &n) in row.iter().enumerate() {
                let border = x == 0 || y == 0 || x == 4 || y == 4;
                assert_eq!(n, border as u8, "({x},{y})");
            }
        }
    }
}
