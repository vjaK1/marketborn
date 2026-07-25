# Test Plan

Testing is part of the product. Everything runs under `npm run check`
(<5 min) or `npm run check:full` (adds slow suites: `--include-ignored`
release tests). Never claim green without running them.

## Gates

| Gate | Contents |
|------|----------|
| `npm run check` | `cargo fmt --check` · frontend build · `clippy --workspace --all-targets -D warnings` · `cargo test --workspace` · `tsc --noEmit` · `vitest run` |
| `npm run check:full` | `check` + `cargo test --workspace --release -- --include-ignored` (soaks; later: proptest, benches) |

## Current suites (Phase 0 + Phase 1 industry slice)

### Unit (sim-core, in-module)

- money: `mul_bp` toward-zero rounding (incl. negatives), i128 overflow
  safety, affordability floor, display.
- rng: substream stability, divergence across name/entity/tick/seed, length
  domain-separation.
- ids: account ordering (businesses before agents), display forms.
- ledger: transfer moves money + journals + conserves; insufficient funds
  rejected without mutation; negative/self-transfer rejected; mint/burn
  adjust the expected total.
- goods_ledger: produce/consume keep expected totals in sync; pantry burns
  hit the food target.
- market: cheapest-offer-first with seller-id tie-break; cash-limited
  purchases conserve money; unmet demand marks seller stockouts; urgent
  buyers ordered by id; tool users buy one per worker under the value cap;
  overpriced tools refused (unmet demand recorded); comfortable households
  shop for the second meal; the wealthy buy exactly one home under the
  half-of-cash cap (overpriced homes refused).
- production: capacity-bound, input-bound, stops at inventory target;
  perishable output targets the smaller larder buffer; equipped workers
  produce the tool bonus; tools wear out and burn through the goods
  ledger; no tools ⇒ no bonus, no wear.
- labor: daily payroll pays every worker; broke business ⇒ missed payroll
  event + workers quit; deterministic vacancy matching; workers switch
  only over the loyalty premium; comfortable ambition holds out and
  desperation accepts; grievances block rehiring until desperation or
  forgetting.
- memory: repetition reinforces instead of duplicating; decay forgets and
  grievances fade below the active threshold; eviction removes the
  weakest first.
- relationships: updates clamp and strangers are neutral; drift fades and
  drops fully-neutral relations; the map is bounded with most-neutral
  eviction; bonds bind and resentment repels (clamped); bonded workers
  stay where identical wages alone would lose them.
- reputation: observation forms beliefs and drift forgets them; gossip
  moves the listener a quarter of the gap (never about themselves); the
  belief store is bounded with most-neutral eviction; hearsay blocks
  hiring of the willing until desperation.
- `probe_reputation` (tests/probe_reputation.rs, pinned seed 42): a
  machinery-produced payroll failure → firsthand victim beliefs → gossip
  carries the news to a non-witness (trajectory-latched; thresholds
  frozen per the probe calibration policy). **Phase 2 acceptance probe —
  passing.**
- consumption: pantry decrement, hunger streaks + events; comfort meal for
  the wealthy, never into hunger; perishables spoil toward zero through the
  goods ledger; durable goods and small stocks untouched.
- decisions: stockout ⇒ price raise; glut ⇒ cut toward floor; idle
  capacity ⇒ cut without glut/stockout; cash crunch fires LIFO; rich
  business pays owner dividend (conserving); a wealthy entrepreneur takes
  over a moribund business (ownership swaps, seller paid, same-tick
  recapitalization, books reconcile through the sale); healthy firms and
  timid money stay put.
- business books: cash identity and operating-profit arithmetic;
  uncategorized business cash flows caught by the business_books invariant
  even when total money is conserved.
- decision engine: neutral traits reproduce the rule family; loss-making
  businesses never cut for volume; identical conditions + different traits
  ⇒ different choices; records render explanations with their inputs;
  reservation wages scale with ambition, decay to zero over the patience
  horizon, and yield to desperation; switch premiums widen with loyalty
  (a 16% raise moves the disloyal, not the loyal); a run of dry windows
  breaks the price deadlock while a single quiet week does not.
- worldgen traits: same seed ⇒ same person; traits vary across the town;
  different seeds ⇒ different people.
- invariants: fresh world green; corrupted cash/inventory/employment each
  caught with a contextual report; out-of-band goods creation and pantry
  edits caught by goods_conservation.
- hashing: equal states hash equal; state change changes hash;
  journal/inputs don't affect hash.
- worldgen: deterministic; town shape (29/10/19, five tool users, goods
  targets seeded); tiny-population clamp valid; 100 agents ⇒ exactly 20
  distinctly-named businesses.
- tick: conservation over 30 ticks; manifest cadence; commands causal and
  tick-exact; past-tick queue rejected; overdraw command ⇒ event not halt;
  halted world refuses to tick; corruption halts with report.
- snapshot: reflects a running world; serializes to JSON; market rows
  cover every good with sane standing depth; balance sheets add up.
- sim-cli: manifest diff helpers (divergence, misaligned cadences).

### Integration (sim-core/tests/)

- `ore_steel_tools_farm_productivity_chain` (industry.rs, 180 days): ore
  bought by the steelworks, steel by the factory, tools by farms/mine; some
  extraction business held tools and out-produced bare-handed capacity;
  wear destroyed tools (bought > held); per-good reconciliation and money
  conservation hold to the end; every business's books reconcile with its
  cash and record real revenue and payroll.
- `wood_bricks_homes_reach_wealthy_households` (construction.rs, 450
  days): materials sold at every stage, homes sold, at least one household
  owns a home; conservation (including owned homes) holds to the end.
- `hundred_agent_twenty_business_year_is_green` (scale.rs) — **the Phase 1
  acceptance run**: 100 agents / 20 businesses, one sim year with
  every-tick invariant sweeps (debug), liveness (employment, meals in
  month twelve), full money/goods/books reconciliation at the end.

### Determinism (sim-core/tests/determinism.rs)

- Twin runs (seed 42, 400 ticks, 2 commands): identical manifests, final
  hash, event streams, metrics, and decision sequences.
- Different seeds diverge.
- Commands are causal (with vs without differ).
- Replay from command log reproduces manifests + hashes + events.
- `soak_1500` *(ignored → check:full)*: no halt, food still produced,
  employment nonzero, food still trading.

### Persistence (sim-persist/tests)

- Roundtrip preserves state hash, journal, command log, pending queue.
- **Resume equality**: run 100 → save → load → run to 250 ≡ uninterrupted
  250 (hashes, manifests, events) — with a pending tick-150 command crossing
  the save boundary.
- Replay-from-save reproduces the saved world and stored manifest.
- Meta/manifest/event/command tables readable; applied flags correct.
- Garbage files, missing files, and future schema versions are clean errors.

### Frontend (vitest)

- Money/date/label formatting (incl. negatives, thousands, calendar
  boundaries).
- Store: snapshot application marks connected, replaces older snapshots,
  speed clamped to shell range.

## Verification beyond tests

Every session that touches the UI or shell: launch the app (or E2E path) and
watch real behavior before claiming it works. CLI runs double as headless
smoke tests (`sim-cli run --seed … --ticks 3650`). Economy changes get
soak checkpoints (years 1/4/10) plus `sim-cli metrics <save> --csv` for
day-by-day time-series analysis — end-state snapshots hide limit cycles.

## Growth map (when phases land)

- **Phase 1**: ~~goods-conservation reconciliation~~ ✅ ·
  ~~ore→steel→tools→farm productivity integration test~~ ✅ ·
  ~~spoilage~~ ✅.
- **Phase 2**: utility scoring units; decision-record storage;
  `probe_reputation`.
- **Phase 3**: contract lifecycle integration; default→foreclosure;
  `probe_rate_shock`; debt reconciliation invariants.
- **Phase 4**: `probe_drought`; `soak_10y` (3650 ticks, non-degeneracy
  band asserts); tax reconciliation; delayed-policy effects.
- **Phase 5**: Playwright E2E against `sim-cli serve` (new world, speed,
  inspect agent/business, apply policy, save, load); packaged-app smoke.
- **Phase 6**: proptest world generators (thousands of ticks, all
  invariants); failure tests (empty markets, no employers, mass bankruptcy,
  bank insolvency, resource exhaustion, extreme inflation, corrupted saves,
  old save versions, 1000-agent worlds); benchmark suite.

## Emergence probes — calibration policy

Probe thresholds (drought price rise, default counts, trust drops) are
calibrated once against a pinned seed when each probe lands, then frozen as
regression guards. Probes assert propagation channels exist — never scripted
outcomes.
