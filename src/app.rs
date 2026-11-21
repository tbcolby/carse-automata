//! State, storage and drawing for Carse Automata. Rules, record formats and
//! timing live in `carse-core`; this file maps keys to them, persists and draws.

use carse_core::codec::{self, Fossil, Meta};
use carse_core::draw::{cursor_frame, light_rect, CELL_PX, FULL};
use carse_core::input::{Pacer, Press, SpaceTracker};
use carse_core::seed::{self, Touch, TouchMap};
use carse_core::{Grid, Sim, CELL_COUNT, GRID_SIZE};
use gam::menu::{PixelColor, Point};
use gam::GlyphStyle;
use precursor_appkit::keys::Key;
use precursor_appkit::rng::Trng;
use precursor_appkit::screen::Screen;
use precursor_appkit::store::Store;

pub const DICT: &str = "carse.data";
const KEY_TOUCHMAP: &str = "touchmap";
const KEY_EPOCH: &str = "epoch.current";
const KEY_META: &str = "epoch.meta";

const GRID_PX: isize = GRID_SIZE as isize * CELL_PX as isize;
/// Blink period in pump ticks (100 ms each): 5 ticks = 2 Hz.
const BLINK_TICKS: u32 = 5;
/// A dead epoch waits this long (ms) before a held space starts the next one.
const DEAD_PAUSE_MS: u64 = 2_000;
/// Save the running epoch every this many generations.
const SAVE_EVERY_TICKS: u32 = 50;
/// Refresh the "last session" time at most this often when nothing else changed.
const META_REFRESH_MS: u64 = 3_600_000;

const STATE_NAMES: [&str; 7] = ["Void", "Dormant", "Cool", "Neutral", "Warm", "Hot", "Burning"];

#[derive(PartialEq, Eq)]
pub enum Action {
    Stay,
    Leave,
}

struct Scrub {
    epoch: u32,
    /// Loaded fossil for `epoch`: `None` = not loaded yet, `Some(None)` = no record.
    loaded: Option<Option<Fossil>>,
    dirty: bool,
}

pub struct App {
    screen: Screen,
    store: Store,
    trng: Trng,
    clock: llio::LocalTime,
    tt: ticktimer_server::Ticktimer,

    sim: Sim,
    touch: TouchMap,
    /// What is currently on screen, so only changed cells are redrawn.
    drawn: Box<Grid>,

    cursor: (usize, usize),
    prev_cursor: (usize, usize),
    cursor_visible: bool,
    cursor_dirty: bool,
    blink: u32,

    space: SpaceTracker,
    pacer: Pacer,
    pending_touch: Option<Touch>,
    dead_since: Option<u64>,
    scrub: Option<Scrub>,

    // persistence
    dirty_state: bool,
    dirty_touch: bool,
    last_saved_tick: u32,
    last_meta_wall: Option<u64>,
    /// Set when saved data couldn't be used; blocks saves so it isn't overwritten.
    read_only: bool,
    status_note: String,

    full_redraw: bool,
    last_status: String,
}

impl App {
    pub fn new(screen: Screen, store: Store, mut trng: Trng) -> Self {
        let touch = TouchMap::new();
        let sim = Sim::fresh(&touch, &mut trng).expect("TRNG failed; refusing to seed from non-random data");
        let mut app = Self {
            screen,
            store,
            trng,
            clock: llio::LocalTime::new(),
            tt: ticktimer_server::Ticktimer::new().expect("carse: no ticktimer"),
            sim,
            touch,
            drawn: Box::default(),
            cursor: (GRID_SIZE / 2, GRID_SIZE / 2),
            prev_cursor: (GRID_SIZE / 2, GRID_SIZE / 2),
            cursor_visible: true,
            cursor_dirty: true,
            blink: 0,
            space: SpaceTracker::default(),
            pacer: Pacer::default(),
            pending_touch: None,
            dead_since: None,
            scrub: None,
            dirty_state: false,
            dirty_touch: false,
            last_saved_tick: 0,
            last_meta_wall: None,
            read_only: false,
            status_note: String::new(),
            full_redraw: true,
            last_status: String::new(),
        };
        app.load(true);
        app
    }

    pub fn screen(&self) -> &Screen { &self.screen }

    fn now_ms(&self) -> u64 { self.tt.elapsed_ms() }

    fn wall_ms(&mut self) -> Option<u64> { self.clock.get_local_time_ms() }

    // ---------------------------------------------------------------- storage

    /// Load saved state. `startup` also applies drift; a basis change just reloads.
    fn load(&mut self, startup: bool) {
        self.read_only = false;
        self.status_note.clear();
        self.pending_touch = None;

        let touch = match self.read_record(KEY_TOUCHMAP, codec::decode_touch) {
            Some(t) => t,
            None => TouchMap::new(),
        };
        let had_epoch;
        match self.read_record(KEY_EPOCH, codec::decode_epoch) {
            Some(e) => {
                had_epoch = true;
                self.sim = Sim::from_parts(Box::new(e.grid), e.epoch, e.tick, e.seed);
            }
            None => {
                had_epoch = false;
                if !startup || self.read_only {
                    // keep whatever is running; nothing readable to replace it
                } else {
                    self.sim = Sim::fresh(&touch, &mut self.trng)
                        .expect("TRNG failed; refusing to seed from non-random data");
                }
            }
        }
        self.touch = touch;
        let meta = self.read_record(KEY_META, codec::decode_meta);
        let wall = self.wall_ms();
        self.last_meta_wall = meta.and_then(|m| m.last_wall_ms);
        if startup && had_epoch && seed::drift_due(self.last_meta_wall, wall) {
            match seed::apply_drift(&mut self.sim.grid, &mut self.trng) {
                Ok(()) => {
                    self.sim.forget_history();
                    self.dirty_state = true;
                }
                Err(_) => log::error!("TRNG failed; drift skipped"),
            }
        }
        self.last_saved_tick = self.sim.tick;
        self.dead_since = None;
        self.full_redraw = true;
        self.cursor_dirty = true;
        if !startup {
            self.dirty_state = false;
            self.dirty_touch = false;
        }
    }

    /// Read and decode one record. A missing key is `None`; an unreadable or
    /// undecodable one is also `None` but turns on `read_only` so it is never overwritten.
    fn read_record<T>(&mut self, key: &str, decode: impl Fn(&[u8]) -> Result<T, codec::DecodeError>) -> Option<T> {
        match self.store.read_bytes(key) {
            Ok(None) => None,
            Ok(Some(bytes)) => match decode(&bytes) {
                Ok(v) => Some(v),
                Err(e) => {
                    log::error!("{}: can't use saved record: {:?}", key, e);
                    self.read_only = true;
                    self.status_note = match e {
                        codec::DecodeError::NewerVersion(_) => "Saved data is from a newer version".into(),
                        _ => "Saved data is damaged; not saving".into(),
                    };
                    None
                }
            },
            Err(e) => {
                log::error!("{}: read failed: {:?}", key, e.kind());
                self.read_only = true;
                self.status_note = "Couldn't read saved data".into();
                None
            }
        }
    }

    fn write(&mut self, key: &str, bytes: &[u8]) -> bool {
        match self.store.write_bytes(key, bytes) {
            Ok(()) => true,
            Err(e) => {
                log::error!("save {} failed: {:?}", key, e.kind());
                self.status_note = "Save failed".into();
                false
            }
        }
    }

    /// Write what changed. Does nothing when nothing is dirty, so pausing,
    /// backgrounding and leaving cost no flash writes (0.1 rewrote everything
    /// on every focus loss and every 100 ms while paused at tick 50, 100, ...).
    fn save(&mut self) {
        if self.read_only {
            return;
        }
        let wall = self.wall_ms();
        let meta_stale = match (self.last_meta_wall, wall) {
            (Some(l), Some(n)) => n.saturating_sub(l) > META_REFRESH_MS,
            (None, Some(_)) => true,
            _ => false,
        };
        if !(self.dirty_state || self.dirty_touch || meta_stale) {
            return;
        }
        if self.dirty_touch {
            let bytes = codec::encode_touch(&self.touch);
            self.dirty_touch = !self.write(KEY_TOUCHMAP, &bytes);
        }
        if self.dirty_state {
            let bytes = codec::encode_epoch(self.sim.epoch, self.sim.tick, self.sim.rules.seed, &self.sim.grid);
            self.dirty_state = !self.write(KEY_EPOCH, &bytes);
            self.last_saved_tick = self.sim.tick;
        }
        let bytes = codec::encode_meta(&Meta { last_wall_ms: wall });
        if self.write(KEY_META, &bytes) {
            self.last_meta_wall = wall;
        }
    }

    fn load_fossil(&self, epoch: u32) -> Option<Fossil> {
        let bytes = self.store.read_bytes(&codec::fossil_key(epoch)).ok().flatten()?;
        match codec::decode_fossil(&bytes) {
            // the ring slot may hold a newer or older epoch
            Ok(f) if f.epoch == epoch => Some(f),
            Ok(_) => None,
            Err(e) => {
                log::warn!("fossil {} unreadable: {:?}", epoch, e);
                None
            }
        }
    }

    pub fn reload(&mut self) {
        self.load(false);
        self.scrub = None;
        self.draw();
    }

    // ------------------------------------------------------------------ input

    pub fn on_keys(&mut self, keys: &[Key]) -> Action {
        if keys.iter().any(|k| k.is_back()) {
            self.commit_touch();
            self.save();
            return Action::Leave;
        }
        let has = |k: Key| keys.contains(&k);
        let space = keys.contains(&Key::Char(' '));
        let (left, right, up, down) = (has(Key::Left), has(Key::Right), has(Key::Up), has(Key::Down));

        // Scrub the fossil record: space + left/right.
        if space && (left || right) {
            self.space.stop();
            let newest = self.sim.epoch.saturating_sub(1);
            let sc = self.scrub.get_or_insert(Scrub { epoch: newest, loaded: None, dirty: true });
            if left && sc.epoch > 0 {
                sc.epoch -= 1;
            }
            if right && sc.epoch < newest {
                sc.epoch += 1;
            }
            sc.loaded = None;
            sc.dirty = true;
            self.draw();
            return Action::Stay;
        }
        if self.scrub.is_some() && !space {
            self.scrub = None;
            self.full_redraw = true;
            self.draw();
            return Action::Stay;
        }

        if space && self.scrub.is_none() {
            let now = self.now_ms();
            match self.space.press(now) {
                Press::Fresh => {
                    self.commit_touch();
                    let idx = self.cursor.1 * GRID_SIZE + self.cursor.0;
                    match seed::touch_cell(&mut self.sim.grid, &mut self.touch, idx, &mut self.trng) {
                        Ok(t) => {
                            self.pending_touch = Some(t);
                            self.dirty_state = true;
                            self.dirty_touch = true;
                            self.sim.forget_history();
                        }
                        Err(_) => self.status_note = "TRNG error: touch ignored".into(),
                    }
                }
                Press::Repeat => {
                    // The tap that started this gesture was the start of a hold, not a touch.
                    if let Some(t) = self.pending_touch.take() {
                        seed::undo_touch(&mut self.sim.grid, &mut self.touch, t);
                    }
                }
            }
            self.draw();
            return Action::Stay;
        }

        // Cursor movement.
        let n = GRID_SIZE;
        let (mut x, mut y) = self.cursor;
        if up {
            y = (y + n - 1) % n;
        }
        if down {
            y = (y + 1) % n;
        }
        if left {
            x = (x + n - 1) % n;
        }
        if right {
            x = (x + 1) % n;
        }
        if (x, y) != self.cursor {
            self.commit_touch();
            self.prev_cursor = self.cursor;
            self.cursor = (x, y);
            self.cursor_visible = true;
            self.cursor_dirty = true;
            self.draw();
        }
        Action::Stay
    }

    /// The pending touch is final once anything else happens.
    fn commit_touch(&mut self) { self.pending_touch = None; }

    // ------------------------------------------------------------------- time

    pub fn on_tick(&mut self) {
        if self.scrub.is_some() {
            return; // nothing animates while browsing fossils
        }
        let now = self.now_ms();
        self.blink += 1;
        if self.blink % BLINK_TICKS == 0 {
            self.cursor_visible = !self.cursor_visible;
            self.cursor_dirty = true;
        }
        self.space.tick(now);
        let held = self.space.running();

        if self.sim.is_dead() {
            let since = *self.dead_since.get_or_insert(now);
            if held && now.saturating_sub(since) >= DEAD_PAUSE_MS {
                self.start_new_epoch();
            }
            self.pacer.steps(now, false);
        } else {
            let steps = self.pacer.steps(now, held);
            for _ in 0..steps {
                self.sim.advance();
                self.dirty_state = true;
                if self.sim.is_dead() {
                    self.dead_since = Some(now);
                    break;
                }
            }
            if self.sim.tick >= self.last_saved_tick + SAVE_EVERY_TICKS {
                self.save();
            }
        }
        self.draw();
    }

    fn start_new_epoch(&mut self) {
        if let Some(reason) = self.sim.death {
            let fossil = Fossil {
                epoch: self.sim.epoch,
                tick_count: self.sim.tick,
                seed: self.sim.rules.seed,
                death: Some(reason),
                energy: self.sim.grid.total_energy(),
                grid: (*self.sim.grid).clone(),
            };
            if !self.read_only {
                let bytes = codec::encode_fossil(&fossil);
                self.write(&codec::fossil_key(fossil.epoch), &bytes);
            }
        }
        match self.sim.next_epoch(&self.touch, &mut self.trng) {
            Ok(()) => {
                self.dirty_state = true;
                self.dead_since = None;
                self.full_redraw = true;
                self.save();
            }
            Err(_) => self.status_note = "TRNG error: no new epoch".into(),
        }
    }

    pub fn on_foreground(&mut self) {
        self.screen.refresh_size();
        self.pacer.steps(0, false);
        self.space.stop();
        self.full_redraw = true;
        self.cursor_dirty = true;
        self.draw();
    }

    pub fn on_background(&mut self) {
        self.space.stop();
        self.pacer.steps(0, false);
        self.save();
    }

    // ---------------------------------------------------------------- drawing

    fn origin(&self) -> Point {
        Point::new((self.screen.size().x - GRID_PX) / 2, 4)
    }

    fn cell_xy(&self, idx: usize) -> (isize, isize) {
        let o = self.origin();
        (o.x + (idx % GRID_SIZE) as isize * CELL_PX as isize, o.y + (idx / GRID_SIZE) as isize * CELL_PX as isize)
    }

    fn rect_at(bx: isize, by: isize, r: (i32, i32, i32, i32)) -> (Point, Point) {
        (Point::new(bx + r.0 as isize, by + r.1 as isize), Point::new(bx + r.2 as isize, by + r.3 as isize))
    }

    /// Full-redraw request from GAM (for example after the status bar changed).
    pub fn redraw(&mut self) {
        self.full_redraw = true;
        if let Some(sc) = self.scrub.as_mut() {
            sc.dirty = true;
        }
        self.draw();
    }

    pub fn draw(&mut self) {
        if self.scrub.is_some() {
            self.draw_scrub();
            return;
        }
        let full = self.full_redraw;
        let mut cells: Vec<usize> = Vec::new();
        if full {
            self.screen.clear();
            let o = self.origin();
            self.screen.fill(o, Point::new(o.x + GRID_PX - 1, o.y + GRID_PX - 1), PixelColor::Dark);
            cells.extend((0..CELL_COUNT).filter(|&i| self.sim.grid.cells[i] != 0));
        } else {
            cells.extend((0..CELL_COUNT).filter(|&i| self.sim.grid.cells[i] != self.drawn.cells[i]));
            if self.cursor_dirty {
                for c in [self.prev_cursor, self.cursor] {
                    let i = c.1 * GRID_SIZE + c.0;
                    if !cells.contains(&i) {
                        cells.push(i);
                    }
                }
            }
        }
        let status = self.status_text();
        let status_changed = full || status != self.last_status;
        if cells.is_empty() && !status_changed && !self.cursor_dirty {
            return;
        }

        if !full && !cells.is_empty() {
            // erase changed cells to the dark background
            let erase = cells.iter().map(|&i| {
                let (bx, by) = self.cell_xy(i);
                Self::rect_at(bx, by, FULL)
            });
            self.screen.fill_rects(erase, PixelColor::Dark);
        }
        let light = cells.iter().filter_map(|&i| {
            let (bx, by) = self.cell_xy(i);
            light_rect(self.sim.grid.cells[i]).map(|r| Self::rect_at(bx, by, r))
        });
        self.screen.fill_rects(light, PixelColor::Light);

        if self.cursor_visible {
            let i = self.cursor.1 * GRID_SIZE + self.cursor.0;
            let (bx, by) = self.cell_xy(i);
            // Light frame on dark cells, dark frame on light ones.
            let color = if self.sim.grid.cells[i] <= 3 { PixelColor::Light } else { PixelColor::Dark };
            self.screen.fill_rects(cursor_frame().iter().map(|&r| Self::rect_at(bx, by, r)), color);
        }
        self.drawn.cells.copy_from_slice(&self.sim.grid.cells);

        if status_changed {
            self.draw_status_lines(&status);
            self.last_status = status;
        }
        self.full_redraw = false;
        self.cursor_dirty = false;
        self.screen.flush();
    }

    fn status_area_top(&self) -> isize { self.origin().y + GRID_PX + 6 }

    fn status_text(&self) -> String {
        let (cx, cy) = self.cursor;
        let state = self.sim.grid.get(cx, cy) as usize;
        let name = STATE_NAMES.get(state).copied().unwrap_or("?");
        let mode = if self.sim.is_dead() {
            match self.sim.death {
                Some(_) if self.sim.tick == 1986 => "The game continues.".to_string(),
                Some(r) => format!("DEAD: {}", r.name()),
                None => String::new(),
            }
        } else if self.space.running() {
            "RUNNING".to_string()
        } else {
            "PAUSED".to_string()
        };
        let note = if self.status_note.is_empty() { String::new() } else { format!("  {}", self.status_note) };
        format!(
            "EPOCH {:05}  TICK {:05}  E:{}\n{}{}\n({},{}) {} [t:{}]",
            self.sim.epoch,
            self.sim.tick,
            self.sim.grid.total_energy(),
            mode,
            note,
            cx,
            cy,
            name,
            self.touch.get(cy * GRID_SIZE + cx)
        )
    }

    fn draw_status_lines(&self, text: &str) {
        let top = self.status_area_top();
        let size = self.screen.size();
        self.screen.fill(Point::new(0, top), Point::new(size.x - 1, size.y - 1), PixelColor::Light);
        let mut y = top;
        for line in text.lines() {
            y += self.screen.text(y, GlyphStyle::Small, line);
        }
        self.screen.text(y, GlyphStyle::Small, "arrows move · space touch / hold · ⌫ back");
    }

    fn draw_scrub(&mut self) {
        let Some(sc) = self.scrub.as_ref() else { return };
        if !sc.dirty {
            return;
        }
        let epoch = sc.epoch;
        let loaded = match &self.scrub.as_ref().and_then(|s| s.loaded.as_ref()) {
            Some(l) => (*l).clone(),
            None => {
                let f = self.load_fossil(epoch);
                if let Some(s) = self.scrub.as_mut() {
                    s.loaded = Some(f.clone());
                }
                f
            }
        };
        self.screen.clear();
        let o = self.origin();
        self.screen.fill(o, Point::new(o.x + GRID_PX - 1, o.y + GRID_PX - 1), PixelColor::Dark);
        let text = match &loaded {
            Some(f) => {
                let light = (0..CELL_COUNT).filter_map(|i| {
                    let (bx, by) = self.cell_xy(i);
                    light_rect(f.grid.cells[i]).map(|r| Self::rect_at(bx, by, r))
                });
                self.screen.fill_rects(light, PixelColor::Light);
                let cause = f.death.map(|d| d.name()).unwrap_or("unknown");
                format!(
                    "FOSSIL  Epoch {:05}  {} ticks\nCause: {}  Energy: {}\n<< left / right to browse, release space to exit >>",
                    f.epoch, f.tick_count, cause, f.energy
                )
            }
            None => format!("FOSSIL  Epoch {epoch:05}\nNo record (never saved or recycled)\n<< left / right to browse, release space to exit >>"),
        };
        let top = self.status_area_top();
        let mut y = top;
        for line in text.lines() {
            y += self.screen.text(y, GlyphStyle::Small, line);
        }
        if let Some(s) = self.scrub.as_mut() {
            s.dirty = false;
        }
        // the live view must repaint fully when scrubbing ends
        self.full_redraw = true;
        self.screen.flush();
    }
}
