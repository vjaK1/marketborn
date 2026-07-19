# Progress

Living state of the project. Updated at the end of every session
(CLAUDE.md protocol). Newest session on top.

---

## Session 2 — 2026-07-19 — Phase 1: COMPLETE

### Where the project stands

**Phase 1 is complete** (all acceptance criteria met, `check:full` green,
no placeholders). Three chains behind one recipe/market machinery: food
(farms → mill → bakery), industry (mine → steelworks → tool factory →
tools as wearing capital), construction (lumber camp + brickworks →
construction co → homes the wealthy buy once). 9 goods; goods conservation
(incl. owned homes) and lifetime business books each guarded by their own
invariant; food spoilage with perishable larders; comfort meals and home
purchases as demand-side hoard recyclers; market view v1 in the UI;
`sim-cli metrics` time-series telemetry; population-scaled worldgen
(DECISIONS #018): 29 agents ⇒ the audited 10-business town, 100 ⇒ exactly
20 businesses, 1,000 ⇒ a ~190-business economy. The Phase 1 acceptance
test (`tests/scale.rs`: 100 agents / 20 businesses / one year / every
invariant green) runs in the regular suite. Next: Phase 2 — agent society.

### What was built

- **Goods & chain**: `Good` grew IronOre/Steel/Tools (appended — market
  order extends, never reshuffles). Three new business kinds staffed in
  worldgen; default town now 26 agents / 7 businesses (staffing
  3+3+3+4+1+1+1, 3 unemployed; money supply $16,200).
- **Goods ledger + invariant** (DECISIONS #012): `goods_ledger` is the only
  creation/destruction doorway (production mints; recipe inputs, meals and
  tool wear burn); `SimState.expected_total_goods` mirrors the money
  targets; `goods_conservation` reconciles per good continuously and halts
  with a report on any bypass (tests corrupt stock and pantries to prove
  it).
- **Tools as capital** (DECISIONS #013): +50% batches per equipped worker;
  6 worker-day life with wear-on-production-days and ledgered breakage;
  buyers pay ≤ 90% of a tool's lifetime marginal product and never invest
  while glutted. `Business` gained `uses_tools`/`tool_wear`;
  `equipped_workers()`/`capacity_batches()` carry the bonus.
- **Demand-side stabilizers** (DECISIONS #014): comfort consumption
  (second meal above $400 cash — closes the hoarding leak that otherwise
  collapses aggregate demand in a closed loop) and idle-capacity pricing
  (profitable single-seller stages cut 2% when selling under half their
  bare-handed capacity — breaks the monopoly ratchet that starved the town
  while the mill profited).
- **UI**: price chart now renders all six series (categorical slots 1–6 of
  the reference palette, validated as a set on this surface; series carry
  direct end labels + legend as the CVD floor-band mitigation); tool stock
  shows in the business rows.
- **Spoilage + telemetry** (DECISIONS #015): food decays 4%/day per holder
  toward zero (remainder stays fresh — pantries never rot), burned through
  the goods ledger, tracked per good in metrics; perishable producers hold
  a 2+1-day larder (covers the mill's 3-day supply oscillation); the
  glut-boundary bug fixed (`GLUT_LIGHT_DAYS` 6 — strictly above the normal
  5-day buffer, which previously made every healthy producer bleed weekly
  price cuts). `MetricsDay` gained per-business daily series and
  `sim-cli metrics <save> --csv` dumps the whole journal — end-state
  snapshots hide limit cycles; this is how the empty-shelf heartbeat and
  the mill's death were actually diagnosed.
- **Business accounting** (DECISIONS #016): lifetime cash-basis `Books` on
  every business (revenue, input/tool costs, wages, dividends, owner
  investment, monetary policy, spoiled units), categorized at the existing
  ledger sites; new `business_books` invariant — cash must equal the
  books' implied cash for every business, every sweep. Statements are
  derived views: snapshot carries the books plus a balance sheet
  (inventory at last market prices), the CLI summary prints lifetime
  operating profit, and the businesses table gained an Assets column.
  Verified zero behavioral impact (year-1 trajectory identical to the
  cent pre/post books).
- **Market view v1**: per-good standing depth (`market::depth` reuses the
  real offer/order-building rules, so the view cannot drift from market
  behavior) + last-day outcomes (volume, unmet, spoilage) in a new
  snapshot `markets` section; Markets panel in the UI (stacked under
  Businesses) with shortage highlighting. Largest buyers/sellers and
  per-good historical charts are Phase 5 polish per BRIEF.
- **Construction chain** (DECISIONS #017): lumber camp → wood, brickworks
  → bricks, construction co (6 wood + 6 bricks → home at $300). Homes are
  one-shot durable assets: a household crossing $600 cash buys one, paying
  ≤ half its cash — after comfort meals, the second hoard-recycling
  channel. Owned homes count in goods conservation; lumber camp and
  brickworks use tools (widening industry demand); homes trade too rarely
  to chart (excluded from the price chart, present in the markets table).
  Population 29, 10 businesses, 9 goods; the year-one housing boom is
  real (~8 homes; construction briefly the most profitable per-worker
  business) and the post-boom idle is the design (see flagged
  limitations).
- **Docs**: ECONOMIC_RULES rewritten for Phase 1 (tool rules, comfort rule,
  utilization pricing, new parameter table with the closed-loop audit);
  DECISIONS #012–#014; TEST_PLAN and PERF_RESULTS updated.

### Actual verification results (all run this session)

- `npm run check`: **exit 0**. 65 sim-core unit + 4 determinism + 2
  integration (industry, construction) + 3 sim-cli + 7 persistence tests
  green; vitest 11/11; fmt/clippy/tsc clean.
- `npm run check:full` (release, `--include-ignored`, incl. soak_1500):
  **exit 0**. (One earlier run hit a transient Windows link-lock on the
  shell's test binary — same AV behavior as session 1's zero-byte save;
  identical target built clean on immediate retry.)
- Integration centerpiece green in debug (invariants every tick, 180 days):
  ore/steel/tool purchases at every stage, bonus production observed, wear
  destroys tools, per-good + money conservation exact.
- Headless soaks with final calibration (release, seed 42, population 29 —
  runtime economics are currently seed-invariant, RNG only names agents):
  **year 1** — food + industry fully staffed (16 employed), the housing
  boom complete (construction sold ~8 homes, $1,367 lifetime profit, then
  idled by design), 18 hungry during the boom-year price discovery.
  **Year 10** — the best long-run equilibrium observed: both farms, mill
  and bakery staffed (13 employed), food $3.51 (below start), 14 of 29
  hungry (the structurally idle), money conserved at $20,700 throughout,
  all invariants green. Industry dies ~year 4 (known limitation);
  construction idles post-boom (the design).
- App launched and inspected four times (screenshots): Y1·D153 with the
  6-series chart and industry dividends; Y1·D56 with the Assets column
  live; Y1·D50 with the Markets panel catching a real shortage day;
  Y1·D66 (three-chain build) with the 8-series chart, 19/19 employed, the
  Home column showing the boom mid-flight (8 homeowners) and construction
  dividends in the event log.
- Perf recorded: 1,000 agents × 3,650 ticks in 0.19 s release
  (PERF_RESULTS.md).

### Flagged limitations (deliberate, recorded — not silent scope reduction)

**Industry-chain long-run persistence.** The chain is healthy through year
one but dies during multi-month wheat-price troughs (tool demand pauses
below the three shops' cash runway; dead businesses have no restart path).
Root causes and fix paths are analyzed in DECISIONS #013: business entry
(Phase 2), credit bridging illiquidity (Phase 3), demand stabilizers
(Phase 4). Phase 1's remaining work does not depend on decade-scale
industry persistence; revisit when those mechanics land.

**Late-game farm monopoly.** In some configurations one farm dies during a
mid-game demand trough and the survivor prices as a monopolist (the
final 29-town run kept both farms alive to year 10 — knife-edge either
way). Same fix family: entry restores competition.

**Construction post-boom idle.** The housing boom exhausts one-shot home
demand in roughly a year; the sector then idles with no restart path
until Phase 2 (by design — DECISIONS #017). Roughly ten calibration
collapses were diagnosed to first causes across this session — the audit
trail lives in DECISIONS #013–#017, and the `sim-cli metrics` CSV
workflow is the tool for the next round.

### Breaking-save-change note (pre-1.0 policy)

Session 1 saves are incompatible: `SimState` gained `expected_total_goods`,
`Business` gained fields, `Good`/`BusinessKind`/recipes changed. All hashes
shift. schema_version stays 1; no released saves exist.

### Scale-up results (the Phase 1 acceptance, run this session)

- `tests/scale.rs` green in debug (every-tick invariants) and release.
- 100-town year 1 (release soak): 39 employed across 20 businesses, real
  multi-firm competition (Cinder & Crumb Bakery fails while three bakeries
  thrive; the lone tool factory prices as a monopolist, $4.5k lifetime
  profit), housing boom completes; money conserved at $54,000. Year 10
  contracts to a 16-employed core — the small-town harsh equilibrium,
  magnified; all invariants green throughout.
- Perf (PERF_RESULTS.md): 1,000 agents / ~190 businesses × 10 sim years =
  0.82 s release (~73× inside the ≤60 s target).

### Exact next task (Phase 2 start — agent society)

1. Read PLAN.md Phase 2 and docs/AGENT_DESIGN.md first. Session protocol:
   `npm run check` before building (green at this commit).
2. First increment suggestion: the utility-based decision engine skeleton —
   action set, deterministic utility scoring (floats allowed in scoring
   only, never accounting), stored decision records with explanations —
   applied first to one existing decision (e.g., the price review) so the
   engine wraps real behavior before new actions (job switching, business
   entry/exit) arrive. Entry/exit is the fix for all three flagged
   limitations; design it against AGENT_DESIGN.md.
3. When touching the economy, verify with the soak checkpoints
   (`sim-cli run --seed 42 --ticks 365/1500/3650 --quiet`, plus
   `--population 100`) and, on any surprise, dump
   `sim-cli metrics <save> --csv` and read the day-by-day series — end
   states hide limit cycles.

Session protocol reminder: `npm run check` first — green at this commit on
`main`.

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
