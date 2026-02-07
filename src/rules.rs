/// The hidden rule system for Carse Automata.
/// Rules are deterministic but non-obvious, derived from a seed.

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
            // Neighbor energy sum needed for void → dormant (8..20)
            birth_threshold: (h(0) % 13 + 8) as u16,
            // Growth zone: energy sum range for +1 state
            growth_min: (h(1) % 8 + 10) as u16,   // 10..17
            growth_max: (h(2) % 10 + 22) as u16,   // 22..31
            // Sustain zone: energy sum range for stable state
            sustain_min: (h(3) % 6 + 4) as u16,    // 4..9
            sustain_max: (h(4) % 12 + 28) as u16,  // 28..39
            // Above this, cells get pushed toward burning
            overheat_threshold: (h(5) % 8 + 34) as u16, // 34..41
            // Total grid energy that triggers collapse death
            collapse_energy: h(6) % 40000 + 40000, // 40k..80k
        }
    }

    /// Compute next state for a single cell given its neighbors.
    pub fn transition(&self, current: u8, neighbors: &[u8; 8]) -> u8 {
        let energy_sum: u16 = neighbors.iter().map(|&n| n as u16).sum();
        let active_count = neighbors.iter().filter(|&&n| n > 0).count() as u8;

        match current {
            0 => {
                // Void: birth requires enough neighbor energy AND enough active neighbors
                if energy_sum >= self.birth_threshold && active_count >= 3 {
                    1
                } else {
                    0
                }
            }
            6 => {
                // Burning: always brief. Collapse based on surroundings.
                if active_count >= 6 && energy_sum >= self.overheat_threshold {
                    // Sustained inferno — stay burning one more tick
                    6
                } else if active_count >= 4 {
                    // Cool down to warm
                    4
                } else {
                    // Collapse to neutral or below
                    (active_count.min(3)) as u8
                }
            }
            s => {
                // States 1-5: grow, sustain, or decay
                if energy_sum >= self.overheat_threshold {
                    // Extreme heat: combustion cascade
                    (s + 2).min(6)
                } else if energy_sum >= self.growth_min && energy_sum <= self.growth_max
                    && active_count >= 2
                {
                    // Growth zone: gradual increase
                    (s + 1).min(6)
                } else if energy_sum >= self.sustain_min && energy_sum <= self.sustain_max {
                    // Sustain zone: stable
                    s
                } else if energy_sum < self.sustain_min || active_count <= 1 {
                    // Too cold or isolated: decay
                    s.saturating_sub(1)
                } else {
                    // Default: slight decay
                    s.saturating_sub(1)
                }
            }
        }
    }

    /// Mutate seed for next epoch using LCG.
    pub fn next_seed(current_seed: u32) -> u32 {
        current_seed.wrapping_mul(1103515245).wrapping_add(12345)
    }
}

/// Death conditions for an epoch.
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
