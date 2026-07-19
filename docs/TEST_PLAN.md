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
  shop for the second meal.
- production: capacity-bound, input-bound, stops at inventory target;
  equipped workers produce the tool bonus; tools wear out and burn through
  the goods ledger; no tools ⇒ no bonus, no wear.
- labor: daily payroll pays every worker; broke business ⇒ missed payroll
  event + workers quit; deterministic vacancy matching.
- consumption: pantry decrement, hunger streaks + events; comfort meal for
  the wealthy, never into hunger.
- decisions: stockout ⇒ price raise; glut ⇒ cut toward floor; idle
  capacity ⇒ cut without glut/stockout; cash crunch fires LIFO; rich
  business pays owner dividend (conserving).
- invariants: fresh world green; corrupted cash/inventory/employment each
  caught with a contextual report; out-of-band goods creation and pantry
  edits caught by goods_conservation.
- hashing: equal states hash equal; state change changes hash;
  journal/inputs don't affect hash.
- worldgen: deterministic; town shape (26/7/16, three tool users, goods
  targets seeded); tiny-population clamp valid.
- tick: conservation over 30 ticks; manifest cadence; commands causal and
  tick-exact; past-tick queue rejected; overdraw command ⇒ event not halt;
  halted world refuses to tick; corruption halts with report.
- snapshot: reflects a running world; serializes to JSON.
- sim-cli: manifest diff helpers (divergence, misaligned cadences).

### Integration (sim-core/tests/industry.rs)

- `ore_steel_tools_farm_productivity_chain` (180 days, default world): ore
  bought by the steelworks, steel by the factory, tools by farms/mine; some
  extraction business held tools and out-produced bare-handed capacity;
  wear destroyed tools (bought > held); per-good reconciliation and money
  conservation hold to the end.

### Determinism (sim-core/tests/determinism.rs)

- Twin runs (seed 42, 400 ticks, 2 commands): identical manifests, final
  hash, event streams, metrics.
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
smoke tests (`sim-cli run --seed … --ticks 3650`).

## Growth map (when phases land)

- **Phase 1**: ~~goods-conservation reconciliation~~ ✅ ·
  ~~ore→steel→tools→farm productivity integration test~~ ✅ · spoilage
  (still to come).
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
