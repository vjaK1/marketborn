# Performance Results

Recorded per PERFORMANCE_PLAN.md. Dev machine: AMD Ryzen 7 7800X3D (8c),
63 GB RAM, Windows 11. Release builds via `cargo build --release`.

## Phase 0 — 2026-07-12

Method: `sim-cli run --seed 42 --ticks 3650 [--population N] --quiet`
(wall-clock measured in the CLI, outside sim-core). Release builds check
invariants on the 50-tick hash cadence.

| Measurement | Result | v1.0 target |
|-------------|--------|-------------|
| 20 agents, 3,650 ticks (10 sim years) | 0.01 s (~317k–735k ticks/s across runs) | — |
| 1,000 agents, 3,650 ticks | **0.08 s** (~46k ticks/s) | ≤ 60 s ✅ (~750× headroom) |
| Save (20 agents, 500 ticks of journal) | < 0.05 s, 307 KB file | ≤ 2 s @ 1,000 agents ✅ (small-world proxy) |
| Replay 500 ticks from save | < 0.01 s (CLI reports 0.00 s) | ≥ live speed ✅ |
| Replay 65-tick UI quicksave | < 0.01 s, hash-exact | — |
| Snapshot cadence with UI open | 10 Hz by construction (shell throttle) | ≤ 10 Hz ✅ |
| UI frame rate | not yet instrumented (Phase 5 measures against the full screen set) | 60 fps |

Notes:

- Debug builds run all invariants every tick and are roughly 2–3× slower —
  still far beyond interactive needs at Phase 0 scale.
- The 1,000-agent run exercises worldgen scaling only (4 businesses, mass
  unemployment); a true 1,000-agent economy lands with Phase 1/2 content.
  Re-measure then.
- No profiling done — nothing is close to a target; per plan, profile before
  optimizing.

## Phase 1 industry slice — 2026-07-19

Same method and machine. Now 6 goods, 7 businesses, goods-conservation
invariant in the sweep, default population 26.

| Measurement | Result | v1.0 target |
|-------------|--------|-------------|
| 26 agents, 3,650 ticks | 0.02 s (~150k–290k ticks/s across runs) | — |
| 1,000 agents, 3,650 ticks | **0.19 s** (~19k ticks/s) | ≤ 60 s ✅ (~300× headroom) |

The 1,000-agent figure remains a worldgen-scaling proxy (7 businesses, mass
unemployment). The extra cost vs Phase 0 (0.08 s → 0.19 s) is the larger
per-tick sweep (more goods, goods reconciliation, comfort demand pass) —
nothing near a target; still no profiling warranted.

## Phase 1 complete (three chains, scaled worldgen) — 2026-07-19

Now 9 goods, business books, spoilage, market depth, and population-scaled
worldgen (DECISIONS #018) — the 1,000-agent world is a real ~190-business
economy, not an unemployment proxy.

| Measurement | Result | v1.0 target |
|-------------|--------|-------------|
| 29 agents / 10 businesses, 3,650 ticks | 0.03 s (~125k–265k ticks/s) | — |
| 100 agents / 20 businesses, 3,650 ticks | 0.06 s (~63k ticks/s) | — |
| 1,000 agents / ~190 businesses, 3,650 ticks | **0.82 s** (~4.5k ticks/s) | ≤ 60 s ✅ (~73× headroom) |

Cost grows roughly with business count (market clearing and reviews are
per-business); still far from any target — no profiling warranted.
