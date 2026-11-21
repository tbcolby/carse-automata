//! The hidden rule system. Rules are deterministic but non-obvious, derived from a seed.

use crate::grid::CELL_COUNT;

/// Smallest and largest collapse thresholds. The grid's maximum energy is
/// `CELL_COUNT * 6 = 24_576`; 0.1 used 40_000..80_000, which no grid can reach,
/// so "Energy Collapse" never happened.
pub const COLLAPSE_MIN: u32 = 9_000;
pub const COLLAPSE_SPAN: u32 = 11_000;
const _: () = assert!(COLLAPSE_MIN + COLLAPSE_SPAN <= (CELL_COUNT as u32) * 6);

#[derive(Clone, Debug)]
pub struct Rules {
    pub seed: u32,
    birth_threshold: u16,
    growth_min: u16,
    growth_max: u16,
    sustain_min: u16,
    sustain_max: u16,
    overheat_threshold: u16,
    pub collapse_energy: u32,
}

impl Rules {
    /// Derive thresholds from a seed using multiplicative hashing.
    pub fn from_seed(seed: u32) -> Self {
        let hash = |s: u32, i: u32| -> u32 {
            let mut h = s.wrapping_mul(2654435761).wrapping_add(i.wrapping_mul(0x9e3779b9));
            h ^= h >> 16;
            h.wrapping_mul(0x85ebca6b)
        };
        let h = |i| hash(seed, i);
        Rules {
            seed,
            birth_threshold: (h(0) % 13 + 8) as u16,
            growth_min: (h(1) % 8 + 10) as u16,
            growth_max: (h(2) % 10 + 22) as u16,
            sustain_min: (h(3) % 6 + 4) as u16,
            sustain_max: (h(4) % 12 + 28) as u16,
            overheat_threshold: (h(5) % 8 + 34) as u16,
            collapse_energy: h(6) % COLLAPSE_SPAN + COLLAPSE_MIN,
        }
    }

    /// Next state for one cell given its neighbours.
    pub fn transition(&self, current: u8, neighbors: &[u8; 8]) -> u8 {
        let energy_sum: u16 = neighbors.iter().map(|&n| n as u16).sum();
        let active_count = neighbors.iter().filter(|&&n| n > 0).count() as u8;

        match current {
            0 => {
                if energy_sum >= self.birth_threshold && active_count >= 3 {
                    1
                } else {
                    0
                }
            }
            6 => {
                if active_count >= 6 && energy_sum >= self.overheat_threshold {
                    6
                } else if active_count >= 4 {
                    4
                } else {
                    active_count.min(3)
                }
            }
            s => {
                if energy_sum >= self.overheat_threshold {
                    (s + 2).min(6)
                } else if energy_sum >= self.growth_min && energy_sum <= self.growth_max && active_count >= 2 {
                    (s + 1).min(6)
                } else if energy_sum >= self.sustain_min && energy_sum <= self.sustain_max {
                    s
                } else {
                    s.saturating_sub(1)
                }
            }
        }
    }

    /// Seed for the next epoch.
    pub fn next_seed(current_seed: u32) -> u32 { current_seed.wrapping_mul(1103515245).wrapping_add(12345) }
}

/// Why an epoch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathReason {
    HeatDeath,
    Stagnation,
    Collapse,
}

impl DeathReason {
    pub fn as_u8(&self) -> u8 {
        match self {
            DeathReason::HeatDeath => 0,
            DeathReason::Stagnation => 1,
            DeathReason::Collapse => 2,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(DeathReason::HeatDeath),
            1 => Some(DeathReason::Stagnation),
            2 => Some(DeathReason::Collapse),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DeathReason::HeatDeath => "Heat Death",
            DeathReason::Stagnation => "Stagnation",
            DeathReason::Collapse => "Energy Collapse",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Audit: collapse_energy was 40_000..80_000 but a grid holds at most 24_576.
    #[test]
    fn collapse_threshold_is_reachable() {
        let max_energy = (CELL_COUNT as u32) * 6;
        let mut seen_lo = u32::MAX;
        let mut seen_hi = 0;
        for seed in 0..5_000u32 {
            let r = Rules::from_seed(seed.wrapping_mul(2654435761));
            assert!(r.collapse_energy >= COLLAPSE_MIN);
            assert!(r.collapse_energy < COLLAPSE_MIN + COLLAPSE_SPAN);
            assert!(r.collapse_energy <= max_energy);
            seen_lo = seen_lo.min(r.collapse_energy);
            seen_hi = seen_hi.max(r.collapse_energy);
        }
        assert!(seen_hi - seen_lo > 5_000, "range is actually used");
    }

    #[test]
    fn from_seed_is_deterministic_and_in_range() {
        let a = Rules::from_seed(0xBEEF);
        let b = Rules::from_seed(0xBEEF);
        assert_eq!(a.collapse_energy, b.collapse_energy);
        for seed in 0..1000u32 {
            let r = Rules::from_seed(seed);
            assert!((8..=20).contains(&r.birth_threshold));
            assert!((10..=17).contains(&r.growth_min));
            assert!((22..=31).contains(&r.growth_max));
            assert!((4..=9).contains(&r.sustain_min));
            assert!((28..=39).contains(&r.sustain_max));
            assert!((34..=41).contains(&r.overheat_threshold));
        }
    }

    #[test]
    fn transition_never_leaves_valid_states() {
        for seed in [1u32, 0xBEEF, 99, 12345] {
            let r = Rules::from_seed(seed);
            for cur in 0..=6u8 {
                for fill in 0..=6u8 {
                    for active in 0..=8usize {
                        let mut n = [0u8; 8];
                        for slot in n.iter_mut().take(active) {
                            *slot = fill;
                        }
                        assert!(r.transition(cur, &n) <= 6);
                    }
                }
            }
        }
    }

    #[test]
    fn void_with_no_neighbours_stays_void_and_isolated_cell_decays() {
        let r = Rules::from_seed(0xBEEF);
        assert_eq!(r.transition(0, &[0; 8]), 0);
        assert_eq!(r.transition(3, &[0; 8]), 2);
        assert_eq!(r.transition(1, &[0; 8]), 0);
    }

    #[test]
    fn void_births_with_enough_active_energetic_neighbours() {
        let r = Rules::from_seed(0xBEEF);
        assert_eq!(r.transition(0, &[6, 6, 6, 6, 0, 0, 0, 0]), 1);
        assert_eq!(r.transition(0, &[6, 6, 0, 0, 0, 0, 0, 0]), 0, "needs three active neighbours");
    }

    #[test]
    fn burning_rules() {
        let r = Rules::from_seed(0xBEEF);
        assert_eq!(r.transition(6, &[6; 8]), 6, "sustained inferno");
        assert_eq!(r.transition(6, &[1, 1, 1, 1, 0, 0, 0, 0]), 4, "cools to warm");
        assert_eq!(r.transition(6, &[1, 1, 0, 0, 0, 0, 0, 0]), 2, "collapses to active count");
        assert_eq!(r.transition(6, &[0; 8]), 0);
    }

    #[test]
    fn death_reason_round_trips() {
        for d in [DeathReason::HeatDeath, DeathReason::Stagnation, DeathReason::Collapse] {
            assert_eq!(DeathReason::from_u8(d.as_u8()), Some(d));
            assert!(!d.name().is_empty());
        }
        assert_eq!(DeathReason::from_u8(9), None);
    }

    #[test]
    fn next_seed_changes() {
        assert_ne!(Rules::next_seed(0xBEEF), 0xBEEF);
    }
}
