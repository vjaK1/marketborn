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
