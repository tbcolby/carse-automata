//! Input timing and pacing from monotonic milliseconds (ticktimer `elapsed_ms`).
//! Nothing here reads a clock; the shell passes `now_ms` in.

/// Key repeat of a held space bar arrives well inside this; a tap doesn't repeat.
pub const HOLD_GAP_MS: u64 = 700;
/// Simulation speed while time flows: one generation per this many ms.
pub const STEP_MS: u64 = 100;
/// Never run more than this many generations for one pump tick (after a stall).
pub const MAX_CATCHUP_STEPS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// First space of a gesture: a touch (undone if it turns out to be a hold).
    Fresh,
    /// A repeat inside the hold window: time is flowing.
    Repeat,
}

/// Space-bar tap vs. hold.
#[derive(Debug, Default, Clone)]
pub struct SpaceTracker {
    last: Option<u64>,
    running: bool,
}

impl SpaceTracker {
    pub fn press(&mut self, now: u64) -> Press {
        let repeat = self.last.is_some_and(|l| now.saturating_sub(l) <= HOLD_GAP_MS);
        self.last = Some(now);
        if repeat {
            self.running = true;
            Press::Repeat
        } else {
            Press::Fresh
        }
    }

    /// Call every pump tick. Returns true if the hold just ended.
    pub fn tick(&mut self, now: u64) -> bool {
        if self.running && self.last.is_none_or(|l| now.saturating_sub(l) > HOLD_GAP_MS) {
            self.running = false;
            return true;
        }
        false
    }

    pub fn running(&self) -> bool { self.running }

    /// Stop time (for example when scrubbing or on focus loss).
    pub fn stop(&mut self) {
        self.running = false;
        self.last = None;
    }
}

/// Converts elapsed monotonic time into generations to run, so the simulation
/// speed doesn't depend on how often the pump actually fires.
#[derive(Debug, Default, Clone)]
pub struct Pacer {
    last: Option<u64>,
    acc: u64,
}

impl Pacer {
    pub fn steps(&mut self, now: u64, running: bool) -> u32 {
        if !running {
            self.last = None;
            self.acc = 0;
            return 0;
        }
        let Some(last) = self.last.replace(now) else {
            return 1; // first tick of a run steps immediately
        };
        self.acc += now.saturating_sub(last).min(STEP_MS * MAX_CATCHUP_STEPS as u64);
        let n = (self.acc / STEP_MS) as u32;
        self.acc %= STEP_MS;
        n.min(MAX_CATCHUP_STEPS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_then_hold() {
        let mut s = SpaceTracker::default();
        assert_eq!(s.press(1_000), Press::Fresh);
        assert!(!s.running());
        assert_eq!(s.press(1_500), Press::Repeat);
        assert!(s.running());
        assert_eq!(s.press(1_550), Press::Repeat);
        assert!(!s.tick(1_700));
        assert!(s.tick(2_300), "hold expires");
        assert!(!s.running());
        assert_eq!(s.press(5_000), Press::Fresh, "long gap is a new tap");
    }

    #[test]
    fn two_slow_taps_are_two_touches() {
        let mut s = SpaceTracker::default();
        assert_eq!(s.press(0), Press::Fresh);
        assert_eq!(s.press(1_500), Press::Fresh);
        assert!(!s.running());
    }

    #[test]
    fn stop_clears_hold() {
        let mut s = SpaceTracker::default();
        s.press(0);
        s.press(100);
        assert!(s.running());
        s.stop();
        assert!(!s.running());
        assert_eq!(s.press(150), Press::Fresh);
    }

    #[test]
    fn pacer_follows_elapsed_time_not_tick_count() {
        let mut p = Pacer::default();
        assert_eq!(p.steps(1_000, true), 1);
        assert_eq!(p.steps(1_100, true), 1);
        // a late pump (250 ms) owes two generations and carries 50 ms
        assert_eq!(p.steps(1_350, true), 2);
        assert_eq!(p.steps(1_450, true), 1, "carry 50 + 100");
        // 10 s stall is capped
        assert_eq!(p.steps(11_450, true), MAX_CATCHUP_STEPS);
        // the 50 ms left over from before the stall is kept; faster-than-STEP_MS ticks accumulate
        assert_eq!(p.steps(11_500, true), 1);
        assert_eq!(p.steps(11_550, true), 0);
        assert_eq!(p.steps(11_600, true), 1);
    }

    #[test]
    fn pacer_resets_when_not_running() {
        let mut p = Pacer::default();
        p.steps(0, true);
        assert_eq!(p.steps(50, false), 0);
        assert_eq!(p.steps(10_000, true), 1, "no catch-up for the paused time");
    }
}
