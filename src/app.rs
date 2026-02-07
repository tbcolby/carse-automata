use gam::menu::*;
use gam::*;
use num_traits::ToPrimitive;

use crate::grid::{Grid, GRID_SIZE, CELL_COUNT};
use crate::rules::{Rules, DeathReason};
use crate::storage::{CarseStorage, EpochMeta};

const CELL_PX: isize = 5;
const GRID_PX: isize = (GRID_SIZE as isize) * CELL_PX; // 320

const FIRST_SEED: u32 = 0xBEEF; // For bunnie.

const STATE_NAMES: [&str; 7] = ["Void", "Dormant", "Cool", "Neutral", "Warm", "Hot", "Burning"];

pub struct CarseApp {
    pub gam: Gam,
    pub gid: Gid,
    pub screensize: Point,
    _token: [u32; 4],

    // Grid state
    pub grid: Grid,
    pub prev_grid: Grid,
    pub prev_prev_grid: Grid,

    // Rules
    pub rules: Rules,
    pub epoch: u32,
    pub tick: u32,

    // Cursor
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub cursor_visible: bool,
    pub blink_counter: u32,

    // Touch map (persistent across epochs)
    pub touch_map: [u32; CELL_COUNT],

    // Simulation state
    pub simulation_running: bool,
    pub last_space_ms: u64,

    // Death state
    pub dead: bool,
    pub dead_ticks: u32,
    pub death_reason: Option<DeathReason>,

    // Fossil scrubbing
    pub scrub_mode: bool,
    pub scrub_epoch: u32,

    // Rendering
    pub grid_origin: Point,
    pub needs_full_redraw: bool,

    // Services
    pub trng: trng::Trng,
    pub storage: CarseStorage,
}

impl CarseApp {
    pub fn new(sid: xous::SID) -> Self {
        let xns = xous_names::XousNames::new().expect("couldn't connect to XousNames");
        let gam = Gam::new(&xns).expect("can't connect to GAM");

        let token = gam
            .register_ux(UxRegistration {
                app_name: String::from(super::APP_NAME),
                ux_type: UxType::Framebuffer,
                predictor: None,
                listener: sid.to_array(),
                redraw_id: super::AppOp::Redraw.to_u32().unwrap(),
                gotinput_id: None,
                audioframe_id: None,
                focuschange_id: Some(super::AppOp::FocusChange.to_u32().unwrap()),
                rawkeys_id: Some(super::AppOp::Rawkeys.to_u32().unwrap()),
            })
            .expect("couldn't register UX")
            .unwrap();

        let gid = gam.request_content_canvas(token).expect("couldn't get canvas");
        let screensize = gam.get_canvas_bounds(gid).expect("couldn't get canvas bounds");

        let trng = trng::Trng::new(&xns).unwrap();
        let storage = CarseStorage::new();

        // Compute grid origin: centered horizontally, near top vertically
        let grid_origin = Point::new(
            (screensize.x - GRID_PX) / 2,
            4,
        );

        // Try to restore state from PDDB
        let (mut grid, epoch, tick, seed, touch_map) = if let Some((e, t, s, g)) = storage.load_epoch_state() {
            let tm = storage.load_touch_map().unwrap_or([0u32; CELL_COUNT]);
            (g, e, t, s, tm)
        } else {
            // Fresh start
            let mut g = Grid::new();
            let tm = storage.load_touch_map().unwrap_or([0u32; CELL_COUNT]);
            g.seed_random(&trng, &tm);
            (g, 0, 0, FIRST_SEED, tm)
        };

        // Apply drift if >24 hours since last session
        if let Some(meta) = storage.load_epoch_meta() {
            let tt = ticktimer_server::Ticktimer::new().unwrap();
            let now = tt.elapsed_ms();
            if now > meta.last_session_ms && (now - meta.last_session_ms) > 86_400_000 {
                apply_drift(&mut grid, &trng);
            }
        }

        let rules = Rules::from_seed(seed);

        CarseApp {
            gam,
            gid,
            screensize,
            _token: token,
            grid: grid.clone(),
            prev_grid: grid.clone(),
            prev_prev_grid: grid,
            rules,
            epoch,
            tick,
            cursor_x: 32,
            cursor_y: 32,
            cursor_visible: true,
            blink_counter: 0,
            touch_map,
            simulation_running: false,
            last_space_ms: 0,
            dead: false,
            dead_ticks: 0,
            death_reason: None,
            scrub_mode: false,
            scrub_epoch: 0,
            grid_origin,
            needs_full_redraw: true,
            trng,
            storage,
        }
    }

    // --- Simulation ---

    pub fn advance_tick(&mut self) {
        // Rotate history
        self.prev_prev_grid = self.prev_grid.clone();
        self.prev_grid = self.grid.clone();

        // Compute next state
        let snapshot = self.prev_grid.clone();
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let neighbors = snapshot.neighbors(x, y);
                let next = self.rules.transition(snapshot.get(x, y), &neighbors);
                self.grid.set(x, y, next);
            }
        }
        self.tick += 1;

        // Check death conditions
        self.check_death();
    }

    fn check_death(&mut self) {
        let energy = self.grid.total_energy();

        if energy == 0 {
            self.dead = true;
            self.death_reason = Some(DeathReason::HeatDeath);
        } else if energy > self.rules.collapse_energy {
            self.dead = true;
            self.death_reason = Some(DeathReason::Collapse);
        } else if self.grid.equals(&self.prev_grid) || self.grid.equals(&self.prev_prev_grid) {
            self.dead = true;
            self.death_reason = Some(DeathReason::Stagnation);
        }
    }

    pub fn start_new_epoch(&mut self) {
        // Save fossil record for the dead epoch
        if let Some(reason) = self.death_reason {
            self.storage.save_fossil(
                self.epoch, self.tick, self.rules.seed,
                reason, self.grid.total_energy(), &self.grid,
            );
        }

        // Save touch map
        self.storage.save_touch_map(&self.touch_map);

        // Easter egg: The Seventh Cell
        let seventh_cell_touches = self.touch_map[7 * GRID_SIZE + 7];

        // Mutate seed
        let new_seed = Rules::next_seed(self.rules.seed);

        // Advance epoch
        self.epoch += 1;
        self.tick = 0;
        self.dead = false;
        self.dead_ticks = 0;
        self.death_reason = None;
        self.rules = Rules::from_seed(new_seed);

        // Seed new grid
        self.prev_grid = Grid::new();
        self.prev_prev_grid = Grid::new();
        self.grid = Grid::new();
        self.grid.seed_random(&self.trng, &self.touch_map);

        // Easter egg: if (7,7) has been touched exactly 49 times, place bridge glider
        if seventh_cell_touches == 49 {
            self.grid.place_bridge_glider(32, 32);
        }

        self.needs_full_redraw = true;
    }

    // --- Touch ---

    pub fn touch_cell(&mut self) {
        let x = self.cursor_x;
        let y = self.cursor_y;
        let idx = y * GRID_SIZE + x;

        // Increment touch count (permanent)
        self.touch_map[idx] = self.touch_map[idx].saturating_add(1);

        // Perturb current state
        let state = self.grid.get(x, y);
        let new_state = match state {
            0 => 1, // Void → Dormant
            1 | 2 | 3 => state + 1, // +1
            4 | 5 => {
                // Random: +1, 0, or -1
                let r = self.trng.get_u32().unwrap_or(0) % 3;
                match r {
                    0 => (state + 1).min(6),
                    1 => state,
                    _ => state - 1,
                }
            }
            6 => 3, // Burning → Neutral
            _ => state,
        };
        self.grid.set(x, y, new_state);
    }

    // --- Input ---

    pub fn handle_keys(&mut self, keys: [char; 4], tt: &ticktimer_server::Ticktimer) {
        let has_space = keys.iter().any(|&k| k == ' ');
        let has_left = keys.iter().any(|&k| k == '\u{2190}');
        let has_right = keys.iter().any(|&k| k == '\u{2192}');
        let has_up = keys.iter().any(|&k| k == '\u{2191}');
        let has_down = keys.iter().any(|&k| k == '\u{2193}');

        // Scrub mode: space + arrow
        if has_space && (has_left || has_right) {
            if !self.scrub_mode {
                self.scrub_mode = true;
                self.simulation_running = false;
                self.scrub_epoch = if self.epoch > 0 { self.epoch - 1 } else { 0 };
            }
            if has_left && self.scrub_epoch > 0 {
                self.scrub_epoch -= 1;
            }
            if has_right {
                let max_epoch = if self.epoch > 0 { self.epoch - 1 } else { 0 };
                if self.scrub_epoch < max_epoch {
                    self.scrub_epoch += 1;
                }
            }
            self.needs_full_redraw = true;
            return;
        }

        // Exit scrub mode on any non-space key
        if self.scrub_mode && !has_space {
            self.scrub_mode = false;
            self.needs_full_redraw = true;
            return;
        }

        // Space: touch + hold detection
        if has_space && !self.scrub_mode {
            let now = tt.elapsed_ms();

            if now.saturating_sub(self.last_space_ms) > 700 {
                // Fresh press — touch the cell
                self.touch_cell();
            } else {
                // Repeated press — simulation running
                self.simulation_running = true;
            }
            self.last_space_ms = now;
            return;
        }

        // Arrow keys: move cursor
        if has_up {
            self.cursor_y = if self.cursor_y == 0 { GRID_SIZE - 1 } else { self.cursor_y - 1 };
        }
        if has_down {
            self.cursor_y = (self.cursor_y + 1) % GRID_SIZE;
        }
        if has_left {
            self.cursor_x = if self.cursor_x == 0 { GRID_SIZE - 1 } else { self.cursor_x - 1 };
        }
        if has_right {
            self.cursor_x = (self.cursor_x + 1) % GRID_SIZE;
        }
    }

    /// Called every pump tick (100ms).
    pub fn pump_tick(&mut self, tt: &ticktimer_server::Ticktimer) {
        // Cursor blink
        self.blink_counter += 1;
        if self.blink_counter % 5 == 0 {
            self.cursor_visible = !self.cursor_visible;
        }

        // Check if space hold has expired
        let now = tt.elapsed_ms();
        if self.simulation_running && now.saturating_sub(self.last_space_ms) > 700 {
            self.simulation_running = false;
        }

        // Handle dead epoch
        if self.dead {
            self.dead_ticks += 1;
            if self.dead_ticks > 20 && self.simulation_running {
                self.start_new_epoch();
            }
            return;
        }

        // Advance simulation
        if self.simulation_running {
            self.advance_tick();
        }

        // Periodic save
        if self.tick > 0 && self.tick % 50 == 0 {
            self.storage.save_epoch_state(self.epoch, self.tick, self.rules.seed, &self.grid);
        }
    }

    // --- Rendering ---

    pub fn draw(&mut self) {
        if self.scrub_mode {
            self.draw_fossil_view();
            return;
        }

        if self.needs_full_redraw {
            self.draw_full_grid();
            self.needs_full_redraw = false;
        } else {
            self.draw_diff();
        }
        self.draw_cursor();
        self.draw_status();
        self.gam.redraw().ok();
    }

    pub fn draw_focus(&mut self) {
        // Clear entire canvas
        self.gam.draw_rectangle(
            self.gid,
            Rectangle::new_coords_with_style(
                0, 0,
                self.screensize.x, self.screensize.y,
                DrawStyle { fill_color: Some(PixelColor::Light), stroke_color: None, stroke_width: 0 },
            ),
        ).ok();
        self.needs_full_redraw = true;
    }

    fn draw_full_grid(&self) {
        // Fill grid background with dark (void)
        self.gam.draw_rectangle(
            self.gid,
            Rectangle::new_with_style(
                self.grid_origin,
                Point::new(self.grid_origin.x + GRID_PX, self.grid_origin.y + GRID_PX),
                DrawStyle { fill_color: Some(PixelColor::Dark), stroke_color: None, stroke_width: 0 },
            ),
        ).ok();

        // Draw all non-void cells
        let mut draw_list = GamObjectList::new(self.gid);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let state = self.grid.get(x, y);
                if state == 0 { continue; } // Void is already dark background
                let rects = cell_rects(self.grid_origin, x, y, state);
                for rect in rects {
                    if draw_list.push(GamObjectType::Rect(rect)).is_err() {
                        self.gam.draw_list(draw_list).ok();
                        draw_list = GamObjectList::new(self.gid);
                        draw_list.push(GamObjectType::Rect(rect)).ok();
                    }
                }
            }
        }
        self.gam.draw_list(draw_list).ok();
    }

    fn draw_diff(&self) {
        let mut draw_list = GamObjectList::new(self.gid);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let new_state = self.grid.get(x, y);
                let old_state = self.prev_grid.get(x, y);
                if new_state == old_state { continue; }

                let rects = cell_rects_full(self.grid_origin, x, y, new_state);
                for rect in rects {
                    if draw_list.push(GamObjectType::Rect(rect)).is_err() {
                        self.gam.draw_list(draw_list).ok();
                        draw_list = GamObjectList::new(self.gid);
                        draw_list.push(GamObjectType::Rect(rect)).ok();
                    }
                }
            }
        }
        self.gam.draw_list(draw_list).ok();
    }

    fn draw_cursor(&self) {
        if !self.cursor_visible { return; }

        let bx = self.grid_origin.x + self.cursor_x as isize * CELL_PX;
        let by = self.grid_origin.y + self.cursor_y as isize * CELL_PX;

        // Determine cursor color: invert of cell background
        let state = self.grid.get(self.cursor_x, self.cursor_y);
        let cursor_color = if state <= 3 { PixelColor::Light } else { PixelColor::Dark };
        let style = DrawStyle { fill_color: Some(cursor_color), stroke_color: None, stroke_width: 0 };

        let mut draw_list = GamObjectList::new(self.gid);
        // Top edge
        draw_list.push(GamObjectType::Rect(Rectangle::new_with_style(
            Point::new(bx, by), Point::new(bx + CELL_PX, by + 1), style,
        ))).ok();
        // Bottom edge
        draw_list.push(GamObjectType::Rect(Rectangle::new_with_style(
            Point::new(bx, by + CELL_PX - 1), Point::new(bx + CELL_PX, by + CELL_PX), style,
        ))).ok();
        // Left edge
        draw_list.push(GamObjectType::Rect(Rectangle::new_with_style(
            Point::new(bx, by + 1), Point::new(bx + 1, by + CELL_PX - 1), style,
        ))).ok();
        // Right edge
        draw_list.push(GamObjectType::Rect(Rectangle::new_with_style(
            Point::new(bx + CELL_PX - 1, by + 1), Point::new(bx + CELL_PX, by + CELL_PX - 1), style,
        ))).ok();
        self.gam.draw_list(draw_list).ok();
    }

    fn draw_status(&self) {
        let status_y = self.grid_origin.y + GRID_PX + 8;

        // Clear status area
        self.gam.draw_rectangle(
            self.gid,
            Rectangle::new_with_style(
                Point::new(0, status_y),
                Point::new(self.screensize.x, self.screensize.y),
                DrawStyle { fill_color: Some(PixelColor::Light), stroke_color: None, stroke_width: 0 },
            ),
        ).ok();

        // Line 1: Epoch and tick
        let mut tv = TextView::new(
            self.gid,
            TextBounds::GrowableFromTl(Point::new(8, status_y + 2), 320),
        );
        tv.style = GlyphStyle::Small;
        use core::fmt::Write;
        write!(tv.text, "EPOCH {:05}  TICK {:05}  E:{}", self.epoch, self.tick, self.grid.total_energy()).ok();
        self.gam.post_textview(&mut tv).ok();

        // Line 2: State
        let mut tv = TextView::new(
            self.gid,
            TextBounds::GrowableFromTl(Point::new(8, status_y + 16), 320),
        );
        tv.style = GlyphStyle::Small;
        if self.dead {
            if let Some(reason) = self.death_reason {
                // Easter egg: 1986 ticks
                if self.tick == 1986 {
                    write!(tv.text, "The game continues.").ok();
                } else {
                    write!(tv.text, "DEAD: {}", reason.name()).ok();
                }
            }
        } else if self.simulation_running {
            write!(tv.text, "RUNNING").ok();
        } else {
            write!(tv.text, "PAUSED").ok();
        }
        self.gam.post_textview(&mut tv).ok();

        // Line 3: Cursor info
        let cell_state = self.grid.get(self.cursor_x, self.cursor_y);
        let touches = self.touch_map[self.cursor_y * GRID_SIZE + self.cursor_x];
        let mut tv = TextView::new(
            self.gid,
            TextBounds::GrowableFromTl(Point::new(8, status_y + 30), 320),
        );
        tv.style = GlyphStyle::Small;
        write!(tv.text, "({},{}) {} [t:{}]",
            self.cursor_x, self.cursor_y,
            STATE_NAMES[cell_state as usize],
            touches
        ).ok();
        self.gam.post_textview(&mut tv).ok();
    }

    fn draw_fossil_view(&mut self) {
        if let Some(fossil) = self.storage.load_fossil(self.scrub_epoch) {
            // Draw fossil grid
            self.gam.draw_rectangle(
                self.gid,
                Rectangle::new_with_style(
                    self.grid_origin,
                    Point::new(self.grid_origin.x + GRID_PX, self.grid_origin.y + GRID_PX),
                    DrawStyle { fill_color: Some(PixelColor::Dark), stroke_color: None, stroke_width: 0 },
                ),
            ).ok();

            let mut draw_list = GamObjectList::new(self.gid);
            for y in 0..GRID_SIZE {
                for x in 0..GRID_SIZE {
                    let state = fossil.grid.get(x, y);
                    if state == 0 { continue; }
                    let rects = cell_rects(self.grid_origin, x, y, state);
                    for rect in rects {
                        if draw_list.push(GamObjectType::Rect(rect)).is_err() {
                            self.gam.draw_list(draw_list).ok();
                            draw_list = GamObjectList::new(self.gid);
                            draw_list.push(GamObjectType::Rect(rect)).ok();
                        }
                    }
                }
            }
            self.gam.draw_list(draw_list).ok();

            // Fossil status
            let status_y = self.grid_origin.y + GRID_PX + 8;
            self.gam.draw_rectangle(
                self.gid,
                Rectangle::new_with_style(
                    Point::new(0, status_y), Point::new(self.screensize.x, self.screensize.y),
                    DrawStyle { fill_color: Some(PixelColor::Light), stroke_color: None, stroke_width: 0 },
                ),
            ).ok();

            let mut tv = TextView::new(
                self.gid,
                TextBounds::GrowableFromTl(Point::new(8, status_y + 2), 320),
            );
            tv.style = GlyphStyle::Small;
            use core::fmt::Write;
            write!(tv.text, "FOSSIL  Epoch {:05}  {} ticks", fossil.epoch, fossil.tick_count).ok();
            self.gam.post_textview(&mut tv).ok();

            let mut tv = TextView::new(
                self.gid,
                TextBounds::GrowableFromTl(Point::new(8, status_y + 16), 320),
            );
            tv.style = GlyphStyle::Small;
            if let Some(reason) = fossil.death_reason {
                write!(tv.text, "Cause: {}  Energy: {}", reason.name(), fossil.energy).ok();
            }
            self.gam.post_textview(&mut tv).ok();

            let mut tv = TextView::new(
                self.gid,
                TextBounds::GrowableFromTl(Point::new(8, status_y + 30), 320),
            );
            tv.style = GlyphStyle::Small;
            write!(tv.text, "<< Left/Right to browse  Release to exit >>").ok();
            self.gam.post_textview(&mut tv).ok();
        }

        self.gam.redraw().ok();
        self.needs_full_redraw = true; // Force full redraw when exiting scrub
    }

    pub fn save_state(&self) {
        self.storage.save_epoch_state(self.epoch, self.tick, self.rules.seed, &self.grid);
        self.storage.save_touch_map(&self.touch_map);
        let tt = ticktimer_server::Ticktimer::new().unwrap();
        self.storage.save_epoch_meta(&EpochMeta {
            epoch_counter: self.epoch,
            last_session_ms: tt.elapsed_ms(),
        });
    }
}

// --- Cell rendering helpers ---

/// Get rects for a cell state (non-void only). Background is assumed dark.
fn cell_rects(origin: Point, x: usize, y: usize, state: u8) -> Vec<Rectangle> {
    let bx = origin.x + x as isize * CELL_PX;
    let by = origin.y + y as isize * CELL_PX;
    let cp = CELL_PX;
    let light = DrawStyle { fill_color: Some(PixelColor::Light), stroke_color: None, stroke_width: 0 };

    match state {
        0 => Vec::new(), // Void = dark background, nothing to draw
        1 => {
            // Dormant: tiny light dot center (1x1)
            vec![Rectangle::new_with_style(
                Point::new(bx + 2, by + 2), Point::new(bx + 3, by + 3), light,
            )]
        }
        2 => {
            // Cool: small light area (2x2 centered)
            vec![Rectangle::new_with_style(
                Point::new(bx + 1, by + 1), Point::new(bx + 3, by + 3), light,
            )]
        }
        3 => {
            // Neutral: medium light area (3x3 centered)
            vec![Rectangle::new_with_style(
                Point::new(bx + 1, by + 1), Point::new(bx + 4, by + 4), light,
            )]
        }
        4 => {
            // Warm: larger light area (4x4)
            vec![Rectangle::new_with_style(
                Point::new(bx, by), Point::new(bx + 4, by + 4), light,
            )]
        }
        5 => {
            // Hot: nearly full (4x5 or 5x4)
            vec![Rectangle::new_with_style(
                Point::new(bx, by), Point::new(bx + cp, by + 4), light,
            )]
        }
        6 => {
            // Burning: full cell light
            vec![Rectangle::new_with_style(
                Point::new(bx, by), Point::new(bx + cp, by + cp), light,
            )]
        }
        _ => Vec::new(),
    }
}

/// Get rects for diff rendering: includes a dark background erase + state pattern.
fn cell_rects_full(origin: Point, x: usize, y: usize, state: u8) -> Vec<Rectangle> {
    let bx = origin.x + x as isize * CELL_PX;
    let by = origin.y + y as isize * CELL_PX;
    let cp = CELL_PX;
    let dark = DrawStyle { fill_color: Some(PixelColor::Dark), stroke_color: None, stroke_width: 0 };

    let mut result = vec![
        // Erase: fill with dark
        Rectangle::new_with_style(Point::new(bx, by), Point::new(bx + cp, by + cp), dark),
    ];
    // Then draw state pattern on top
    result.extend(cell_rects(origin, x, y, state));
    result
}

/// Apply drift: slight random perturbation after long absence.
fn apply_drift(grid: &mut Grid, trng: &trng::Trng) {
    for i in 0..CELL_COUNT {
        if grid.cells[i] > 0 {
            let r = trng.get_u32().unwrap_or(0);
            if r % 20 == 0 {
                // 5% chance of +-1
                if r & 0x100 != 0 {
                    grid.cells[i] = (grid.cells[i] + 1).min(6);
                } else {
                    grid.cells[i] = grid.cells[i].saturating_sub(1);
                }
            }
        }
    }
}
