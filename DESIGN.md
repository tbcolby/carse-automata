# CARSE AUTOMATA
## A Finitely Infinite Game for the Precursor Platform

```
"There are at least two kinds of games. 
One could be called finite, the other infinite.
A finite game is played for the purpose of winning,
an infinite game for the purpose of continuing the play."

— James P. Carse, Finite and Infinite Games (1986)
```

---

## Overview

**Carse Automata** is a cellular automaton sandbox designed for the Precursor secure computing platform. It is not a game to be won. It is a game to be played.

The simulation runs in **epochs**—finite lifespans of emergent complexity that arise, evolve, and collapse. When an epoch ends, a new one begins. The player's attention carries forward. The state does not.

You are not a god. You are a gardener who can stop time.

---

## Platform

- **Device**: Precursor (bunnie's open hardware secure computing platform)
- **Display**: Monochrome/limited color, 336x536 nominal
- **Input**: D-pad + Spacebar only
- **Storage**: Persistent touch map and epoch history via PDDB
- **Power**: Battery-aware; simulation sleeps when device sleeps

---

## Core Concepts

### The Grid

A finite, toroidal grid. Edges wrap—there are no boundaries, only topology.

| Parameter | Value |
|-----------|-------|
| Dimensions | 64x64 (tunable) |
| Topology | Toroidal (wrap both axes) |
| Cell states | 7 (see States below) |

The grid size is constrained by display and performance. 64x64 = 4,096 cells. Enough for emergence. Small enough for intimacy.

### States

Each cell exists in one of **seven states**, representing energy levels:

| State | Name | Color/Glyph | Behavior |
|-------|------|-------------|----------|
| 0 | Void | Black | Absent. Does not participate. |
| 1 | Dormant | Deep blue | Stable. Hard to excite. |
| 2 | Cool | Blue | Low energy. Slowly decays. |
| 3 | Neutral | Gray | Balanced. Most reactive to neighbors. |
| 4 | Warm | Yellow | Elevated energy. Spreads influence. |
| 5 | Hot | Orange | High energy. Volatile. |
| 6 | Burning | White | Maximum energy. Brief. Collapses or spreads. |

Why seven? 

*"And on the seventh state, it rested."*

### Rules

The transition rules are **hidden from the player**.

Each tick, a cell's next state is determined by:
- Its current state
- The states of its 8 neighbors (Moore neighborhood)
- A deterministic but non-obvious function

The rules are discoverable through observation but never disclosed. The player is a scientist, not an engineer.

**Rule Mutation**: Between epochs, the ruleset may mutate slightly. The player cannot control this. The player may not even notice.

### Epochs

An **epoch** is one continuous run of the simulation from seed to death.

**Death conditions**:
- **Heat death**: All cells reach Void or Dormant (entropy wins)
- **Stagnation**: Grid enters a static or period-2 loop for N ticks
- **Collapse**: Total energy exceeds threshold, cascade to Void

When an epoch ends:
1. The final state is recorded
2. The epoch is added to the **Fossil Record**
3. A new epoch seeds
4. The **Touch Map** persists

**Epochs are finite. The chain of epochs is infinite.**

### The Touch Map

Every cell has a **touch count**: the number of times the player has pressed spacebar on it, across all epochs, across all time.

The Touch Map never resets.

When a new epoch seeds:
- Cells with higher touch counts are more likely to seed in higher energy states
- The player's accumulated attention shapes the initial conditions of every future epoch

**You are not controlling the game. You are biasing the probability space.**

### Time

Time flows at the player's command.

| Input | Effect |
|-------|--------|
| **Tap spacebar** | Touch the cell under cursor. Increment touch count. May perturb current state. |
| **Hold spacebar** | Time flows. Simulation runs at ~10 ticks/second. |
| **Release spacebar** | Time stops. |

When the app is closed, time does **not** advance. The simulation waits.

*But it remembers how long you were gone.*

If more than 24 hours pass between sessions, the simulation may apply **drift**—a slight random perturbation to the current state, as if the world dreamed while you were away.

---

## Inputs

### Hardware

- **D-pad**: Cardinal movement (up/down/left/right)
- **Spacebar**: Primary action

No other inputs are used. No menus. No modes. No settings.

### Controls

| Input | Context | Action |
|-------|---------|--------|
| D-pad | Default | Move cursor |
| Spacebar (tap) | Default | Touch cell at cursor |
| Spacebar (hold) | Default | Run simulation |
| D-pad | While holding spacebar | Scrub through time (see Fossil Record) |

### The Cursor

The cursor is a single cell highlight. It does not affect the simulation by proximity—only by touch.

The cursor's position persists across sessions.

---

## Fossil Record

The simulation maintains a **Fossil Record**: a compressed history of all epochs.

Each epoch entry contains:
- Epoch number
- Seed state hash
- Duration (in ticks)
- Death condition
- Peak energy
- Final state snapshot
- Player touches during epoch

### Time Scrubbing

While holding spacebar, the D-pad changes function:

| Input | Action |
|-------|--------|
| D-pad left/right | Move between epochs |
| D-pad up/down | Scrub within current epoch's history |

The player can witness any moment in any epoch. The player cannot change the past.

*"You can't step in the same river twice. But you can remember the river."*

---

## Visual Design

### Display Layout

```
┌────────────────────────────────────────┐
│                                        │
│                                        │
│              64x64 GRID                │
│           (centered, scaled)           │
│                                        │
│                                        │
├────────────────────────────────────────┤
│ EPOCH: 00047  │  TICK: 00000  │ ░░░░░░ │
└────────────────────────────────────────┘
```

- **Grid**: Centered, maximum size that fits
- **Status bar**: Bottom edge only
  - Epoch number
  - Tick count (within epoch)
  - Energy bar (total grid energy, visualized)

No other UI elements. The grid is the interface.

### Color Palette

Constrained by Precursor display. Assume 4-bit color (16 colors) or monochrome with dithering.

If monochrome:
- Void = black
- States 1-6 = increasing density dither patterns
- Cursor = inverse/blink

If color available:
- Cool spectrum (blue) for low energy
- Warm spectrum (yellow/orange/white) for high energy
- Cursor = contrasting highlight

### Cursor Rendering

The cursor should be visible but unobtrusive:
- Single cell highlight
- Blink rate: 500ms
- Does not obscure cell state (use border, not fill)

---

## Storage

### PDDB Keys

| Key | Contents | Persistence |
|-----|----------|-------------|
| `carse:touchmap` | 64x64 array of u32 touch counts | Forever |
| `carse:epoch:current` | Current epoch state | Session |
| `carse:epoch:meta` | Epoch counter, last session timestamp | Forever |
| `carse:fossil:{n}` | Compressed epoch history | Forever (pruned if space required) |
| `carse:rules` | Current ruleset seed | Per-epoch |

### Storage Budget

Assume conservative PDDB allocation:
- Touch map: 64 * 64 * 4 = 16 KB
- Current state: 64 * 64 * 1 = 4 KB
- Epoch meta: 64 bytes
- Fossil record: ~1 KB per epoch, keep last 100 = 100 KB
- **Total**: ~120 KB base + growth

---

## Simulation Engine

### Tick Loop

```
fn tick(grid: &mut Grid, rules: &Rules) {
    let snapshot = grid.clone();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let neighbors = snapshot.moore_neighbors(x, y);
            let current = snapshot.get(x, y);
            let next = rules.transition(current, neighbors);
            grid.set(x, y, next);
        }
    }
}
```

### Rule System

The rule system is intentionally opaque. Implementation suggestion:

```
fn transition(current: State, neighbors: [State; 8]) -> State {
    let neighbor_sum: u32 = neighbors.iter().map(|s| s.energy()).sum();
    let neighbor_count: u32 = neighbors.iter().filter(|s| s.is_active()).count();
    
    // Hidden function combining:
    // - Current state
    // - Neighbor sum (total energy nearby)
    // - Neighbor count (population density)
    // - Internal rule seed
    
    apply_rule(current, neighbor_sum, neighbor_count, self.seed)
}
```

The `apply_rule` function should be:
- Deterministic (same inputs = same outputs)
- Non-obvious (not easily reverse-engineered)
- Capable of producing emergence (gliders, oscillators, chaos)

**Reference**: Wolfram's elementary cellular automata, but extended to multi-state with energy model.

### Death Detection

Check after each tick:

```
fn check_death(grid: &Grid, history: &History) -> Option<DeathCondition> {
    let total_energy = grid.total_energy();
    
    // Heat death
    if total_energy == 0 {
        return Some(DeathCondition::HeatDeath);
    }
    
    // Stagnation (static or period-2)
    if history.is_repeating(grid, 2) {
        return Some(DeathCondition::Stagnation);
    }
    
    // Collapse (energy explosion)
    if total_energy > COLLAPSE_THRESHOLD {
        return Some(DeathCondition::Collapse);
    }
    
    None
}
```

---

## Touch Mechanics

### Touch Effect

When the player touches a cell:

1. **Touch count increments** (permanent, in Touch Map)
2. **State perturbation** (temporary, current epoch only):
   - If Void → becomes Dormant
   - If Dormant/Cool/Neutral → gains +1 state
   - If Warm/Hot → random: +1, 0, or -1
   - If Burning → collapses to Neutral

Touch is not control. Touch is *influence*.

### Touch-Biased Seeding

When a new epoch seeds:

```
fn seed_epoch(touch_map: &TouchMap) -> Grid {
    let mut grid = Grid::new();
    let max_touch = touch_map.max();
    
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let touch_weight = touch_map.get(x, y) as f32 / max_touch as f32;
            let base_probability = 0.15; // 15% chance of non-void
            let boosted_probability = base_probability + (touch_weight * 0.35);
            
            if random() < boosted_probability {
                let state = weighted_random_state(touch_weight);
                grid.set(x, y, state);
            }
        }
    }
    
    grid
}
```

Cells you've attended are more likely to begin alive.

---

## Easter Eggs

### The Seventh Cell

If you touch the cell at coordinates **(7, 7)** exactly **49 times** (7 × 7), the next epoch will seed with a pattern.

The pattern is a glider. But not just any glider.

It's shaped like a **bridge**.

### The Carse Quote

If an epoch lasts exactly **1986** ticks (the year *Finite and Infinite Games* was published), the death message changes from the standard text to:

```
"The game continues."
```

### For Maelee

Hidden in the Fossil Record serialization format, there is a comment:

```
// For M.D.C. — the infinite game that matters most.
```

### The Precursor Homage

The rule seed for the very first epoch on a fresh install is not random.

It is **0xBEEF**.

For bunnie.

---

## Implementation Phases

### Phase 1: Core Grid
- Grid data structure
- State representation
- Basic rendering

### Phase 2: Simulation
- Tick loop
- Hidden rule system
- Death detection

### Phase 3: Input
- Cursor movement
- Touch mechanic
- Time flow (hold spacebar)

### Phase 4: Persistence
- Touch map save/load
- Epoch state save/load
- Session timestamp tracking

### Phase 5: Epochs
- Death → reseed cycle
- Touch-biased seeding
- Epoch counter

### Phase 6: Fossil Record
- History compression
- Time scrubbing UI
- Epoch browsing

### Phase 7: Polish
- Visual refinement
- Status bar
- Drift mechanic
- Easter eggs

---

## Philosophy

Carse Automata is not a productivity tool. It is not a game you win. It is not a screensaver.

It is a **practice space**.

A place to:
- Observe without controlling
- Influence without directing
- Attend without expecting
- Continue without finishing

The finite player asks: "How do I win?"
The infinite player asks: "How do I keep playing?"

Carse Automata has only one answer:

**Press spacebar.**

---

## Appendix A: Reference Material

- Carse, James P. *Finite and Infinite Games*. 1986.
- Wolfram, Stephen. *A New Kind of Science*. 2002.
- Huang, Andrew "bunnie". *The Hardware Hacker*. 2017.
- Precursor documentation: https://precursor.dev
- Xous OS documentation: https://xous.dev

---

## Appendix B: The Name

**Carse**: For James P. Carse, who articulated the distinction between finite and infinite games.

**Automata**: For the cellular automata that form the substrate of the simulation.

Together: **Carse Automata**—a class of automata that play infinite games.

This is not a game about cells.

This is a game about you.

---

```
CARSE AUTOMATA v0.1
Designed by Tyler Colby & Claude
For the Precursor Platform
2026

"What will you attend to?"
```
