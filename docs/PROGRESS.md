# Progress

Living state of the project. Updated at the end of every session
(CLAUDE.md protocol). Newest session on top.

---

## Session 1 — 2026-07-12 — Phase 0 vertical slice: COMPLETE

### Where the project stands

Phase 0 is done end to end. The workspace, quality gates, docs set,
simulation kernel, minimal food-chain economy, SQLite persistence,
determinism suite, headless CLI and the desktop app all exist, all verified
against real runs — no placeholders.

### What was built

- **Workspace & gates**: Cargo workspace (`sim-core`, `sim-persist`,
  `sim-cli`, `src-tauri`) + `app/` (React 18, Vite 6, TS strict, Zustand,
  ECharts). `npm run check` = fmt · app build · clippy `-D warnings` · cargo
  tests · tsc · vitest; `check:full` adds release `--include-ignored`
  (soak). Toolchain pinned 1.97.0; lockfiles committed. Rust/Node installed
  on this machine this session (rustup + VS Build Tools 2022 via winget).
- **Kernel** (`sim-core`): tick orchestrator with the 11-phase order
  (ECONOMIC_RULES.md); `Money(i64)` cents + basis-point math; stateless
  ChaCha8 substreams (DECISIONS #002); command queue/log with `(tick, seq)`
  application; ledger (transfer/mint/burn — the only money doorway); event +
  transaction + metrics journal (ring-buffered); invariants (money
  conservation, non-negative cash/inventory, employment reciprocity) with
  halt-and-report; postcard→BLAKE3 state hashing with manifest (DECISIONS
  #001/#003).
- **Phase 0 economy**: 20 agents (4 owners / 11 jobs / 5 unemployed), two
  farms → mill → bakery, posted-price markets with deterministic clearing,
  daily payroll, weekly staggered price/wage/dividend reviews, owner capital
  injections, producer reservation prices (DECISIONS #004/#011).
- **Persistence**: single-transaction SQLite saves (world blob + meta +
  commands/events/manifest tables), load, replay-from-save, schema-version
  guard, stale-sidecar cleanup.
- **CLI**: `sim-cli run/replay/hash/diff` — diff reports first divergent
  manifest tick with command/event context from both saves.
- **Desktop app**: sim thread owning the world; mpsc inbound, 10 Hz
  `snapshot` events outbound; speed levels pause/2/10/50/max; Save button →
  `%APPDATA%/com.marketborn.app/saves/quicksave.mbsave`; dashboard UI
  (stats, validated-palette price chart, business/agent tables, event log).

### Actual verification results (all run this session)

- `npm run check`: **exit 0**. Rust: 46 sim-core unit + 4 determinism +
  3 sim-cli + 7 persistence tests green; vitest 11/11; tsc/clippy/fmt clean.
- `check:full` (release, `--include-ignored`): **exit 0**, incl.
  `soak_1500_ticks_stays_alive_and_green`.
- Determinism: twin runs (400 ticks + 2 commands) hash/event/metric
  identical; save@100→resume→250 ≡ uninterrupted 250 (with a pending
  command crossing the boundary); replay-from-log and replay-from-save
  hash-exact; UI-produced quicksave replayed by CLI: **hash-exact**.
- App launched twice (before/after economy tuning), screenshots inspected:
  living economy — price competition, firings, dividends, hunger events,
  boom/bust visible; money supply pinned at $10,800 throughout. Save button
  exercised via keyboard automation; file verified + replayed.
- Perf recorded in PERF_RESULTS.md: 1,000 agents × 3,650 ticks in 0.08 s
  release (target ≤ 60 s).

### Economy behavior notes (expected, by design)

Long runs settle into a harsh equilibrium: seed 42 at tick 3650 has one farm
dead (emergent monopoly), 8 employed, stable ~2× prices, and the
structurally unemployed hungry — coherent for a world with no welfare
(Phase 4), no business entry (Phase 2), no death/migration. Early-game
(first ~1–2 sim years) shows rich dynamics: price wars, cash crunches,
recapitalizations, a mid-crisis around D130–200 that clears by ~D400.
DECISIONS #011 records the stabilizers that prevent the two absorbing
collapse states found during tuning.

### Known rough edges (none block Phase 0)

- Businesses table needs a horizontal scrollbar tweak (cash column clipped
  at default width) — cosmetic, Phase 5 UI pass.
- Bakery owners can starve beside full shelves (no self-consumption from own
  business inventory; owners buy at market like everyone). Revisit with
  Phase 2 agent needs.
- `hash --at` advances a loaded save but there is no UI load path yet
  (save-slot management is Phase 5 per PLAN.md).
- Save archives only the in-memory journal rings (50k events); fine at this
  scale, revisit archival append strategy when event volume grows.
- Windows icon is a placeholder (DECISIONS #010).

### Breaking-save-change note (pre-1.0 policy)

Save blobs from before this session's economy tuning (if any existed) are
incompatible — `TxKind`/`Event` gained variants and `Business` gained
fields. schema_version stays 1; no released saves exist.

### Exact next task (Phase 1 start)

1. Read PLAN.md Phase 1. Extend `Good` with the industry chain (iron ore,
   steel, tools) behind the existing recipe machinery: new business kinds
   (mine, steel mill, tool factory) in worldgen with calibrated parameters
   (follow the ECONOMIC_RULES §Phase 0 calibration table as the pattern —
   add a Phase 1 table).
2. Add the goods-conservation invariant (production/consumption/trade
   reconciliation per good per tick) — TEST_PLAN.md already reserves it.
3. Then: tools → farm/mine productivity effect, and the
   ore→steel→tools→farm-productivity integration test (Phase 1's
   acceptance centerpiece).

Session protocol reminder: `npm run check` first — it was green at commit
`(this commit)` on branch `main`.
