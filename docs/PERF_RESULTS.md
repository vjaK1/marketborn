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

## Phase 6 perf pass — 2026-08-16

The full simulation now (phases 2–5 content: decisions, contracts,
bank, government, shocks). The failure-suite measurement found the
first real regression: pop-1000 decade at **268.7 s (13 ticks/s
average)** — 4.5× OVER the 60 s target, and aging (175 t/s in year one
decaying as businesses died).

**Profiled first** (temporary per-phase instrumentation, DECISIONS
#041): the decisions phase was 98% of tick time (74,209 µs of ~75,500
at t2000) and grew with the dead-business count. Root cause: each
weekly takeover reviewer's live-demand gate called `market::depth()` —
a full walk of every offer and order — per moribund business; cheap
while the town is staffed, quadratic once most of a 191-business town
is dead. Fix: memoize the per-good demand answer per tick, invalidated
whenever an executed takeover mutates state — decisions bit-identical
(verified: identical business-level dumps pre/post at pop-1000 t2000,
identical pop-29 matrix endpoints, shared metrics columns identical
through the decade).

| Measurement | Result | v1.0 target |
|-------------|--------|-------------|
| 29 agents, 3,650 ticks | 0.10–0.15 s (~30k ticks/s) | — |
| 1,000 agents / 191 businesses, 3,650 ticks | **3.06 s** (~1,190 ticks/s) — was 268.7 s | ≤ 60 s ✅ (~20× headroom) |
| Load at 1,000 agents (13.6 MB decade save, incl. re-hash) | **0.64 s** | ≤ 2 s ✅ |
| Save at 1,000 agents | **0.63 s** | ≤ 2 s ✅ |
| Replay 3,650 ticks at 1,000 agents (verify vs manifest) | 3.30 s sim (~1,106 ticks/s) | ≥ live ✅ — 93% of headless max (the delta is per-cadence hash verification), 22× any paced live speed |
| Memory | ring-bounded by construction; decade pop-1000 save = 13.6 MB | bounded ✅ |
| UI snapshot cadence | 10 Hz shell/serve throttle by construction | ✅ |

Remaining watch item (not near target): per-tick cost still scales
with business count via market clearing; the terminal contract/loan
maps measured small (80 / 262 entries after a pop-1000 decade) — the
suspected accumulation was innocent, the takeover×depth interaction
was the whole story.
