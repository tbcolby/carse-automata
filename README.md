# Carse Automata

**A finitely infinite game for the Precursor platform.**

```
"There are at least two kinds of games.
One could be called finite, the other infinite.
A finite game is played for the purpose of winning,
an infinite game for the purpose of continuing the play."

— James P. Carse, Finite and Infinite Games (1986)
```

---

## What This Is

Carse Automata is a cellular automaton sandbox that runs on [Precursor](https://precursor.dev) — bunnie Huang's open-source, auditable secure computing handheld. It is not a game you win. It is a game you play.

A 64x64 toroidal grid of cells evolves through seven energy states according to hidden transition rules. The simulation runs in **epochs** — finite lifespans of emergent complexity that arise, evolve, and inevitably collapse. When one epoch dies, a new one begins. Your attention carries forward. The state does not.

You move a cursor. You touch cells. You hold spacebar and time flows. You release it and time stops. That is the entire interface.

**You are not a god. You are a gardener who can stop time.**

---

## Why This Project

Two books made this inevitable.

**James P. Carse** wrote [*Finite and Infinite Games*](https://en.wikipedia.org/wiki/Finite_and_Infinite_Games) in 1986. It is a slim, strange book about the difference between playing to win and playing to continue. Finite players play within boundaries. Infinite players play with boundaries. Most software is a finite game — you accomplish a task, you close the app, the game is over. Carse Automata refuses that framing. There is no score, no objective, no win state. There is only the next epoch.

**Stephen Wolfram** wrote [*A New Kind of Science*](https://www.wolframscience.com/nks/) in 2002 — a thousand-page argument that simple computational rules generate irreducible complexity. That you cannot predict what Rule 110 will do without running it. That the universe is, in some deep sense, a cellular automaton. Carse Automata takes this seriously: the transition rules are hidden from the player. You are a scientist, not an engineer. You observe. You hypothesize. You touch a cell and watch what happens.

The intersection of these ideas — *infinite play* and *computational irreducibility* — demanded a physical artifact. Not a website. Not a phone app. Something you hold in your hands, whose hardware you can audit, whose software you can read.

That artifact is Precursor.

---

## Why Precursor

Precursor is bunnie Huang's answer to a question most people haven't thought to ask: *what would a computer look like if you could actually trust it?*

It is an open-source handheld with a RISC-V CPU, a physical keyboard, a 336x536 monochrome display, and 16 MiB of RAM. Its operating system, [Xous](https://xous.dev), is a microkernel that communicates entirely through message passing. There is no shared memory. There are no file descriptors. Every process is an island.

These constraints are not limitations — they are the game's boundaries, and boundaries are what make play possible.

The monochrome display forced us to render seven cell states as graduated fill patterns — from solid dark (Void) to solid light (Burning), with increasingly large light rectangles for states in between. A 1-bit display has no grayscale, but the eye reads density as brightness. The constraint became the aesthetic.

The 100 MHz CPU and no GPU meant we couldn't redraw 4,096 cells every frame. So we diff: only cells that changed between ticks get redrawn, batched into draw lists of 32 rectangles at a time. The constraint became the architecture.

The keyboard-only input meant no touch screen, no mouse, no gestures. Just arrows and spacebar. So spacebar became everything: tap to touch a cell, hold to run time, hold + arrows to scrub through fossil records of dead epochs. The constraint became the interaction.

And Precursor's encrypted storage (PDDB — the Plausibly Deniable DataBase) meant the Touch Map — a permanent record of every cell you've ever attended to, across all epochs — lives in hardware-encrypted flash. Your accumulated attention is private by default. The constraint became the philosophy.

### For bunnie

The first epoch's rule seed on a fresh install is `0xBEEF`. A small nod to the person who built the hardware that makes this game possible, who has spent a career arguing that open hardware is a prerequisite for trustworthy computing, and who would probably rather we ship something interesting than write long dedications.

---

## How It Works

### The Grid

64 x 64 cells on a toroidal surface. The edges wrap — there are no boundaries, only topology. Each cell exists in one of seven energy states:

| State | Name | Visual (1-bit) | Energy |
|-------|------|-----------------|--------|
| 0 | Void | Solid dark | 0 |
| 1 | Dormant | 1px light center | 1 |
| 2 | Cool | 2x2 light center | 2 |
| 3 | Neutral | 3x3 light center | 3 |
| 4 | Warm | 4x4 light fill | 4 |
| 5 | Hot | Nearly full light | 5 |
| 6 | Burning | Solid light | 6 |

Why seven? *"And on the seventh state, it rested."*

### Controls

| Input | Action |
|-------|--------|
| Arrow keys | Move cursor |
| Spacebar (tap) | Touch cell: perturb state, increment permanent touch count |
| Spacebar (hold) | Time flows at ~10 ticks/second |
| Space + Left/Right | Scrub through fossil record of past epochs |

No menus. No modes. No settings.

### Hidden Rules

The transition rules are deterministic but hidden. Each tick, a cell's next state depends on its current state, its eight neighbors' states, and a seed-derived threshold system. The player discovers the rules through observation, never through documentation.

Between epochs, the rule seed mutates. The rules change. The player may not even notice.

*Reference: Wolfram's elementary cellular automata, extended to multi-state with an energy model.*

### Epochs

An epoch is one continuous run from seed to death. Death comes in three forms:

- **Heat Death**: Total energy reaches zero. The universe is cold.
- **Stagnation**: The grid enters a static state or period-2 oscillation. Nothing new can happen.
- **Collapse**: Total energy exceeds a threshold. Everything burns out at once.

When an epoch dies, a fossil record is saved and a new epoch begins. The Touch Map carries forward — cells you've attended to in past epochs are more likely to start alive in future ones.

**Epochs are finite. The chain of epochs is infinite.**

### The Touch Map

Every cell has a touch count: the number of times you've pressed spacebar on it, across all epochs, forever. The Touch Map never resets.

When a new epoch seeds, cells with higher touch counts are biased toward higher energy initial states. You are not controlling the simulation. You are biasing the probability space of every future epoch's initial conditions.

### Drift

If more than 24 hours pass between sessions, the simulation applies *drift* — slight random perturbation to the current state, as if the world dreamed while you were away.

### Easter Eggs

There are four. One is visible in the source. Three require play to discover.

---

## Technical Architecture

```
apps/carse/
├── Cargo.toml       # Dependencies: xous, gam, trng, pddb, ticktimer
└── src/
    ├── main.rs      # Entry point, pump thread, event loop
    ├── app.rs       # Game state, rendering, input, simulation
    ├── grid.rs      # 64x64 toroidal grid, neighbors, seeding
    ├── rules.rs     # Hidden rule system, seed-derived thresholds
    └── storage.rs   # PDDB persistence, RLE compression, fossils
```

### Design Decisions

**Framebuffer UX** (`UxType::Framebuffer`): Raw pixel control, following the ball app pattern. No text chrome, no managed widgets — just rectangles on a canvas.

**Pump thread pattern**: A dedicated thread sends blocking scalar messages to the main loop at 100ms intervals (10 ticks/sec). The main loop acknowledges each pump, providing natural backpressure. Copied directly from the ball app — this is the canonical Xous animation pattern.

**Diff rendering**: A full grid redraw pushes up to 4,096 cells x 2 rectangles each through the GAM. Diff rendering only redraws changed cells, batched into `GamObjectList` calls (32 items max per batch). Full redraws happen only on focus change and epoch start.

**RLE compression for fossils**: Fossil records store the final grid state of each epoch. Since grids tend to have spatial locality (runs of same-state cells), run-length encoding compresses the 4,096-byte grid to typically 500-2,000 bytes. The circular buffer holds the last 100 epochs.

**Hidden rules via seed-derived thresholds**: Rather than encoding complex lookup tables, the rule system derives numeric thresholds from the seed using a multiplicative hash. This produces varied but deterministic behavior across epochs — some seeds create rich emergence, others produce rapid collapse.

### PDDB Storage Layout

| Key | Size | Persistence |
|-----|------|-------------|
| `touchmap` | 16 KB (4096 x u32) | Forever |
| `epoch.current` | ~4.1 KB (header + grid) | Per session |
| `epoch.meta` | 12 bytes | Forever |
| `fossil.{0-99}` | ~0.5-2 KB each (RLE) | Circular |

Total footprint: ~120 KB base + fossil growth.

---

## Building

Carse Automata is a Xous app. It builds as part of the [xous-core](https://github.com/betrusted-io/xous-core) workspace.

### Integration

1. Copy `src/` and `Cargo.toml` to `xous-core/apps/carse/`

2. Add to workspace `Cargo.toml`:
   ```toml
   "apps/carse",
   ```

3. Add to `apps/manifest.json`:
   ```json
   "carse": {
       "context_name": "Carse Automata",
       "menu_name": {
           "appmenu.carse": {
               "en": "Carse Automata",
               "en-tts": "Carse Automata"
           }
       }
   }
   ```

4. Build for Renode emulator:
   ```bash
   cargo xtask renode-image carse
   ```

5. Build for hardware:
   ```bash
   cargo xtask app-image carse
   ```

### Dependencies

```toml
[dependencies]
xous = "0.9.69"
xous-ipc = "0.10.9"
gam = { path = "../../services/gam" }
trng = { path = "../../services/trng" }
pddb = { path = "../../services/pddb" }
ticktimer-server = { package = "xous-api-ticktimer", version = "0.9.68" }
log-server = { package = "xous-api-log", version = "0.1.68" }
xous-names = { package = "xous-api-names", version = "0.9.70" }
num-derive = { version = "0.4.2", default-features = false }
num-traits = { version = "0.2.14", default-features = false }
log = "0.4.14"
```

---

## Screenshots

*Captured via headless Renode emulation on macOS ARM64.*

> Screenshots will be added after the first Renode capture run.
> Use: `python3 scripts/renode_capture.py --app carse --screenshots screenshots/`

---

## The Name

**Carse**: For James P. Carse, who articulated the distinction between finite and infinite games.

**Automata**: For the cellular automata that form the substrate of the simulation.

Together: **Carse Automata** — a class of automata that play infinite games.

---

## Philosophy

Carse Automata is not a productivity tool. It is not a game you win. It is not a screensaver.

It is a **practice space**.

A place to observe without controlling. To influence without directing. To attend without expecting. To continue without finishing.

The finite player asks: *"How do I win?"*
The infinite player asks: *"How do I keep playing?"*

Carse Automata has only one answer:

**Press spacebar.**

```
CARSE AUTOMATA v0.1
Designed by Tyler Colby & Claude
For the Precursor Platform
2026

"What will you attend to?"
```

---

## Development
---

This app was developed using the methodology described in [xous-dev-toolkit](https://github.com/tbcolby/xous-dev-toolkit) — an LLM-assisted approach to Precursor app development on macOS ARM64.

## Author
---

Made by Tyler Colby — [Colby's Data Movers, LLC](https://colbysdatamovers.com)

Contact: [tyler@colbysdatamovers.com](mailto:tyler@colbysdatamovers.com) | [GitHub Issues](https://github.com/tbcolby/carse-automata/issues)

## License
---

Licensed under the Apache License, Version 2.0.

See [LICENSE](LICENSE) for the full text.
