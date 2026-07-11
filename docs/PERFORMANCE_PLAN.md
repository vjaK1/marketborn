# Performance Plan

Targets are release builds on the dev machine (Windows 11, the primary
platform). Numbers get recorded in `PERF_RESULTS.md` at every phase.
**Profile before optimizing** — evidence goes in DECISIONS.md before any
major optimization.

## v1.0 targets (from BRIEF.md)

| Target | Threshold |
|--------|-----------|
| 100 agents, UI open | UI 60 fps while sim runs at max speed; snapshots ≤ 10 Hz |
| 1,000 agents headless | 3,650 ticks (10 sim years) ≤ 60 s |
| Save / load at 1,000 agents | ≤ 2 s each |
| Replay | ≥ live simulation speed |
| Memory | bounded: ring buffers in memory, SQLite archive; no full-world clone per tick; no DB writes in the tick loop |

## Measurement methods

- **Headless throughput**: `sim-cli run --seed 42 --ticks 3650 [--population N]`
  prints ticks/s (wall-clock in the CLI, never in core).
- **Save/load/replay**: `sim-cli` timings + persistence test instrumentation.
- **UI**: devtools frame profiling with the sim at max speed (Phase 5, when
  the screen set stabilizes); snapshot rate is clamped shell-side at 10 Hz by
  construction.
- **Benchmark suite** (Phase 6): tick duration p50/p99, decisions/s, memory
  RSS, event throughput, save/load/replay times, UI render time.

## Design provisions already in place

- Journal ring buffers (events 50k / tx 10k / metrics 4k) bound memory.
- Snapshots are compact summaries; detail queries (Phase 2+) fetch by id.
- Single-threaded core by contract — parallelism is a post-1.0 project.
- Persistence is a single transaction outside the tick loop.

## Known future hotspots (watch, don't pre-optimize)

- Market matching is O(offers × orders) per good — fine at Phase 0/1 scale;
  revisit with 1,000 agents (sorted structures already in place).
- Postcard-hash of full state each cadence — measure at 1,000 agents;
  incremental hashing only with profiling evidence.
- Event text rendering in snapshots is per-capture — cap is 120 rows.
