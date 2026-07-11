# Architecture

## Workspace

```
crates/sim-core      pure simulation: no I/O, no wall clock, no threads, no unsafe
crates/sim-persist   SQLite save/load/replay/event archive (outside the tick loop)
crates/sim-cli       headless binary: run · replay · hash · diff
src-tauri            desktop shell: hosts the simulation thread, Tauri 2 IPC
app                  React + Vite + TypeScript frontend (Zustand, ECharts)
docs                 binding documentation
```

Dependency direction: `app → (IPC) → src-tauri → sim-persist → sim-core` and
`sim-cli → sim-persist → sim-core`. Nothing depends on the shell; the same
core powers desktop, headless and test modes.

## The World type (sim-core)

`World` splits into three parts with different contracts:

| Part | Contents | Hashed? | Saved? |
|------|----------|---------|--------|
| `SimState` | tick, config, agents, businesses, market state, expected money total, status | **yes** | yes |
| `InputLog` | full command log + pending queue + seq counter | no (inputs are the run's cause, not its state — DECISIONS.md #003) | yes |
| `Journal` | events, transactions, daily metrics, hash manifest | no (outputs; compared explicitly in determinism tests) | yes |

Everything that can influence future simulation outcomes lives in `SimState`
and nowhere else. `Journal` is write-only from simulation code — systems never
read it.

## Determinism machinery

- **RNG**: `ChaCha8Rng` substreams derived per use from BLAKE3 of
  `(master seed, stream name, entity id, tick)` — stateless, so the master
  seed in config is the complete RNG state (DECISIONS.md #002).
- **Iteration**: entities live in `BTreeMap`s keyed by stable integer ids;
  every outcome-affecting loop is id-ordered or explicitly sorted.
- **Hashing**: postcard-serialize `SimState` → BLAKE3 hex (DECISIONS.md #001).
  Manifest entries at tick 0, every `hash_every` ticks, and at every save.
- **Commands**: queued for strictly future ticks, logged verbatim, applied at
  phase 1 in `(tick, seq)` order. Replay = worldgen(config) + command log.

## Tick pipeline

`World::tick()` runs the 11-phase order pinned in ECONOMIC_RULES.md. Phases
are free functions over `(&mut SimState, &mut Journal, tick)` — no system
holds state of its own. A `DayAccumulator` threads per-tick trade/consumption
statistics into the metrics capture.

Invariant failure → `SimStatus::Halted` + `TickError::Invariant(report)`;
the world refuses further ticks until a fresh load.

## Persistence (sim-persist)

One SQLite file per save (`.mbsave`), written in a single transaction:

- `world` — postcard blob of the entire `World` (authoritative for load)
- `meta` — schema_version, tick, seed, config JSON, state hash, app version
- `commands` / `events` / `manifest` — queryable side tables for `sim-cli
  diff` and the event archive

Load = read blob. Replay = regenerate from meta.config + requeue `commands`.
Schema versions above the supported one are refused with a clean error.
SQLite is never touched inside the tick loop; the shell and CLI call
`save()` from outside.

## Desktop shell (src-tauri)

One background thread owns the `World` (`marketborn-sim`). The UI thread and
webview never touch simulation state.

```
UI (React) ──invoke──▶ tauri commands ──mpsc──▶ sim thread
   ▲                                              │
   └────── `snapshot` events (≤10 Hz) ◀───emit────┘
```

- Inbound `ShellMsg`: `SetSpeed(0..=4)`, `Save(reply)`. Player commands will
  flow through the same channel as `PlayerCommand`s and are queued at tick
  boundaries (machinery exists in core; Phase 0 exposes no command UI).
- Outbound: `WorldSnapshot` (compact summary: stats, agent/business rows,
  price history tail, event tail — never the full world) throttled to 10 Hz.
  A `get_snapshot` command pulls the latest on startup.
- Speed levels: 0 pause · 1 = 2 t/s · 2 = 10 t/s · 3 = 50 t/s · 4 = max
  (tick back-to-back; snapshots stay throttled). Pacing is shell-side wall
  clock — never inside sim-core.
- Saves go to `<app-data>/saves/quicksave.mbsave` via sim-persist.

`sim-cli serve` (websocket, Phase 5) will implement this same protocol for
Playwright E2E; the frontend is transport-agnostic behind `app/src/ipc.ts`.

## Frontend (app)

- `ipc.ts` — the only module that talks to Tauri (dynamic imports so the
  bundle also loads in a plain browser and in vitest).
- `store.ts` — Zustand store: latest snapshot, connection state, speed.
- `App.tsx` — dashboard: header (date, speed controls, save), stat chips,
  price chart (ECharts, palette validated for the dark surface), business
  table, event log, agent table.
- No simulation logic in components; the UI renders snapshots and sends
  commands, nothing else.

## Memory bounds

In-memory ring buffers (journal): events 50k, transactions 10k, metrics 4k
days. Saves archive the current buffers; SQLite is the long-term archive.
No full-world cloning per tick anywhere.
