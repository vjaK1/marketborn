# Progress

Living state of the project. Updated at the end of every session
(CLAUDE.md protocol). Newest session on top.

---

## Session 8 — 2026-08-16 — Phase 4 increments 2–3: shocks + the test skeleton

### Scenario shocks v1 (`d68169d`) — DECISIONS #030

- `shocks.rs`: deterministic condition modifiers riding the existing
  command log — `TriggerShock { kind, days }` (clamped 1..=3,600)
  activates at its tick boundary, lives in hashed state, and the
  long-reserved tick phase 2 retires it the day it expires (exactly
  `days` modified production days). One shock per kind; re-triggering
  is a `CommandRejected`. `ShockBegan`/`ShockEnded` events.
- ONE mechanical hook: `capacity_bp(state, kind)`, applied at BOTH the
  production batch cap and the price review's utilization base — a
  drought-throttled farm neither overproduces nor reads its withered
  fields as idle capacity (which would fire price CUTS into the
  scarcity). Drought: farms yield half.
- **`probe_drought` ✅** (pinned seed 42): injected at tick 600 — the
  mature steady state, where the cut BINDS. Calibrated and frozen:
  output 56% of control (guard ≤75%), wheat peak +69% over its
  pre-drought mean while control stays flat (guard ≥130% of both),
  food +28% (guard ≥115%), 34 food-chain price raises (guard ≥5).
  First calibration at tick 200 was absorbed by the post-boom glut —
  recorded as the system working, and why the probe binds at 600.

### The Phase 4 test skeleton is complete — DECISIONS #031

- **`soak_10y` ✅** (check:full): 3,650 command-free ticks, every-tick
  invariants, non-degeneracy bands calibrated to the real steady
  state: food produced and trading at the end; ≥1 staple repricing in
  the last 500 ticks (wheat: 3 distinct prices); ≥1 roster death + ≥1
  revival across the decade (actual 12/6 — "exit and entry" mapped to
  the economy's real churn channels, since v1 never founds/deletes
  businesses); employment in band (actual 13/6). Runs in ~0.8s.
- **`policy_lag` ✅** (the delayed-policy-effect test): the honest
  delayed effect is welfare ABOLITION, not a hike — in the mature
  steady state both seed 42 and marginal seed 123 absorb even a 9%
  sales tax indefinitely (the dole recycles the whole take into final
  demand; business cash ends HIGHER — redistribution, not
  contraction). `SetSalesTax{0}` at tick 600: the dole stops in days,
  hunger stays within +0.15 of control for a quarter, then climbs
  +5.7 by [1100,1500) — the no-welfare equilibrium arriving on a
  ~500-tick fuse. Frozen: ≤1.0 divergence through the first quarter,
  ≥3.0 by year three.
- Both gates exit 0 (136 sim-core unit tests; check:full incl.
  soak_1500 + soak_10y). The four-seed decade soak reproduces session
  7's endpoints EXACTLY — an untriggered shock system is
  behavior-neutral. Saves break again (SimState grew `shocks`).

### Phase 4 scorecard

- Done-when: ~~probe_reputation~~ ✓ · ~~probe_rate_shock~~ ✓ ·
  ~~probe_drought~~ ✓ · ~~soak_10y~~ ✓ — **all four probes + soak
  green**, plus tax reconciliation and the delayed-policy test.
- Remaining phase scope (mechanics, not tests): the "all policy
  levers" set and government budget/debt.

### Exact next task (Phase 4 close-out)

1. Session protocol: `npm run check` first.
2. **Scope "all policy levers" for v1** (a DECISIONS entry): the BRIEF
   lists business/income/sales tax, interest rate, spending, subsidies,
   welfare, minimum wage, antitrust, contract enforcement, bankruptcy
   rules, import/export, emergency relief. Existing: sales tax, bank
   rate, money supply, shocks. Suggested v1 close-out set — welfare
   floor as a settable lever (`SetWelfareFloor`), minimum wage
   (`SetMinimumWage`, floors the wage walk-down), and **government
   budget/debt** (the phase's named scope: let the treasury borrow
   from the bank or issue debt so spending can exceed intake — with
   the delayed-policy machinery already proven, a deficit lever
   completes the fiscal loop). Levers whose mechanics don't exist
   (import/export, antitrust, bankruptcy-rule variation) get recorded
   as post-1.0 or Phase-6-failure-test scope.
3. Then declare Phase 4 complete (check:full + soak matrix + PLAN.md
   Delivered block), and Phase 5 (UI completion + `sim-cli serve` +
   Playwright E2E) opens.
4. Soak checkpoints unchanged: seeds 42/7/123/6 at 365/1500/3650 +
   pop-100; metrics CSV on surprises.

---

## Session 7 — 2026-08-16 — Phase 4 increment 1: the government kernel

### Government kernel v1 — DECISIONS #029

- `government.rs`: a first-class ledger account (`AccountId::Government`,
  appended after Bank), **born broke** — the treasury holds only what
  taxation collected. One tax end to end: a seller-side sales tax
  (integer bp, round-toward-zero, sub-cent remainder stays with the
  seller) collected at BOTH revenue sites — market execution and
  contract settlement — booked as `Books.taxes_paid` on the payer and in
  the government's lifetime `GovBooks`. Exempt: liquidations, penalties,
  wages, dividends, business sales (future levers).
- **Welfare floor** in a new tick phase 8 (banking → government →
  consumption; CLAUDE.md's pinned phase order updated with the ADR):
  agents below $12.00 (~2 days of food) are topped up daily, most
  destitute first, until the treasury runs dry. `SetSalesTax` command
  (clamped 0..=10,000 bp) + `SalesTaxSet`/`WelfarePaid` events +
  metrics/CSV columns (govt_cash, tax_collected, welfare_paid,
  welfare_recipients).
- **`tax_reconciliation` invariant** (every sweep): treasury == books;
  fiscal totals non-negative; rate inside the clamp; Σ business
  `taxes_paid` == `tax_collected`. It caught its first real bug on
  contact: `probe_reputation`'s books-resync surgery silently wiped tax
  history (staging now carries it forward — the invariant note documents
  the pattern).

### The 300 bp lesson (why the default is 100 bp)

First calibration (300 bp) collapsed seeds 123 and 6 to dead towns by
year 4 while 42/7 merely inflated. Bisected by experiment (rate 0 ==
baseline exactly on all four seeds; welfare disabled isolates the tax):
seed 6 died of the **cascading wedge alone** (a turnover tax hits wheat,
flour AND food — ≈3× the rate on food's final value); seed 123 died of
the **dole's final-demand prop** (recycled cash held food/wheat prices
up while the mill's margin collapsed between them — it bled out beside
a farm holding 22 unsold wheat, then the staffed bakery starved of
flour). At 100 bp every seed holds the baseline's 13 employed. A
pantry-targeted dole was tried and REJECTED on measurement (no relief
vs the plain floor; seed 42 six agents worse). Full story in ADR #029.

### Verification (actual results)

- `npm run check` exit 0 (132 sim-core unit tests incl. 4 new
  government + 2 new invariant tests; `taxes.rs` integration suite ×4;
  all prior suites green after two honest updates: the contract
  delivery unit test now expects the seller to net gross−tax, and
  probe_reputation's staging preserves tax history).
- Release soak, final config (tax 100 bp, floor $12): seeds 42/7/123/6
  at 3650 → **13 employed each** (baseline preserved); last-500-tick
  mean hunger 14.2/19.9/19.5/20.1 vs baseline ≈19.4–20.3 (seed 42
  clearly better, others neutral). Steady state: treasury pins at $0,
  ~0.9 recipients/day, ~$7,000 redistributed per decade.
- Pop-100: 1-year acceptance (`scale.rs`) green; decade run 13/100
  employed — top of the recorded 7–13 band, **still open** (a 1% dole
  cannot fix a structural scaling problem; owned by `soak_10y` +
  welfare work later this phase).
- Saves break (SimState grew `government`; Books/TxKind/Event/
  PlayerCommand grew) — pre-1.0 policy, schema_version stays 1.

### Exact next task (Phase 4 continues)

1. Session protocol: `npm run check` first.
2. **The deterministic event system** (tick phase 2, reserved since
   Phase 0): scheduled events that modify CONDITIONS only — drought
   first (cuts farm productivity for a window; food scarcity, price
   response and recovery must emerge from normal systems), wired for
   `probe_drought` (calibrate once against a pinned seed, then freeze).
   Suggested shape: event definitions in config/commands (deterministic
   scheduling, no RNG outside substreams), an `EventState` in SimState,
   a production-side modifier hook, events + snapshot text.
3. Then: delayed-policy-effect test (the 300 bp collapse is a ready-made
   scenario — raise the tax by command mid-run, assert the lagged
   contraction), then `soak_10y` with non-degeneracy bands, then the
   remaining levers opportunistically (government debt, income/business
   tax, minimum wage, subsidies; welfare floor as a settable lever).
4. Soak checkpoints unchanged: seeds 42/7/123/6 at 365/1500/3650 +
   pop-100; metrics CSV on surprises.

---

## Session 6 — 2026-07-26 — Phase 3: COMPLETE (negotiation + contract view)

### Negotiation v1 — DECISIONS #028

- `negotiation.rs`: a bounded three-round integer haggle on supply
  formation, anchored in observable state (no RNG): buyer opens 6–12%
  under posted (greed stretches), seller floors 2–8% under (greed
  narrows), explicit convergence rules, impasse when the floor exceeds
  the buyer's earning ceiling. Every offer/counteroffer/reason journals
  into a new `Journal.negotiations` ring (cap 2,000) — the BRIEF's "log
  every negotiation completely". The ACHIEVED discount (not a flat 5%)
  feeds the Sign/StaySpot review; outcomes: Impasse, BuyerDeclined (won
  the table, lost the review), Signed.
- The flat `CONTRACT_DISCOUNT_BP` is gone; contract prices now vary
  with the two owners' greed — traits move real money.

### Contract view (the Phase 3 UI deliverable)

- Snapshot carries the newest 50 contracts (parties, good, ceiling,
  price, tallies, state chip); `get_contract_detail` serves the
  inspector over the reserved on-demand protocol: terms, tallies,
  penalties, the negotiation move by move, and the contract's event
  history (honestly bounded by the events ring).
- **Launch-verified**: at day 26 live delivery events stream in the
  log; at year 88 the table shows breached/completed states; clicking
  the breached steel contract opened its inspector — signed Y1·D282,
  15/84 delivered, missed 3, penalties $5.64, and the full haggle from
  "buyer opened below spot — $7.17" to "seller gave the bottom line —
  $7.53 / buyer accepted — $7.53". Incidental: the debug shell ran
  ~260k consecutive every-tick-invariant ticks at max speed without a
  halt; the pop-29 world holds 13/19 employed through simulated
  centuries.

### Phase 3 declared complete

- All three acceptance criteria green (supply contract E2E;
  default→foreclosure; probe_rate_shock) + negotiation logging +
  contract view. `npm run check` and `check:full` exit 0 (130+ unit
  tests across 9 suites; vitest 11; tsc clean).
- **Recorded scope tradeoffs** (PLAN + DECISIONS #026–#028, not
  silent): of the BRIEF's seven contract types, supply ships end to
  end; employment rides the existing labor machinery, loans are the
  bank's paper, acquisitions exist as takeover deals; partnerships/
  exclusive-distribution/leases plus deposits, mortgages, wage
  negotiation, and food-chain contracts are recorded deferrals tied to
  their driver phases. The dedicated negotiation inspector is v1.1 by
  the BRIEF's own line.

### Exact next task (Phase 4 — government, events, emergence)

1. Session protocol: `npm run check` first; read PLAN Phase 4 +
   BRIEF's government/events sections.
2. Suggested first increment: the government kernel — a Government
   account (like the bank), ONE tax end to end (e.g. sales tax at the
   market site: integer bp, remainder-assigned, `tax_reconciliation`
   invariant), a budget that spends on something real (welfare floor —
   which also owns the recorded pop-100 decade issue), and the
   `SetTaxRate`-style command plumbing. Then the deterministic event
   system (drought first, for `probe_drought`), then `soak_10y`.
3. Soak checkpoints unchanged: seeds 42/7/123/6 at 365/1500/3650 +
   pop-100; metrics CSV on surprises.

---

## Session 5 — 2026-07-26 — Phase 3 increment 2: the bank (credit kernel)

### Bank v1 — DECISIONS #027

- `bank.rs`: one bank with a first-class ledger account
  (`AccountId::Bank`), capitalized at worldgen ($70/resident, minted
  into the money supply), its own lifetime books, and the new
  `debt_reconciliation` invariant (bank cash == books; per-loan
  balance/counter/state identities; loan-book sums == aggregates) every
  sweep.
- 84-day working-capital term loans: straight-line principal, fixed
  annual rate (360-day year), interest accrued daily in integer
  milli-cents on the declining balance (sub-cent carry never becomes
  money until paid). Service collects daily in tick phase 7,
  interest-first, full-or-miss; three consecutive misses default →
  foreclosure: cash seized, then goods at last market prices into bank
  inventory (goods conservation counts it), remainder written off
  against equity; seized goods fire-sell daily to the market's own
  deterministic buyer queue. The stripped business survives for the
  takeover machinery.
- Demand side on the utility engine (`BorrowReview`, the distress
  ladder's third rung after own till and owner injection): payroll
  runway = urgency, rate = price, debt aversion = weight. Supply side
  deterministic: no second loan, no prior defaulters (v1 credit
  memory), a 25% liquidity floor (defaults shrink the lendable pool —
  the BRIEF's credit-contraction capability), income OR coverage test.
  `SetBankRate` player command reprices future loans (clamped 0–500%).
  Loan service is junior to wages/market/contracts and protected in the
  borrower's market budget.

### Phase 3 acceptance scorecard — all three criteria GREEN

- Supply contract negotiated + fulfilled end-to-end ✓ (session 4).
- **default→foreclosure flow ✓** (`foreclosure.rs`): staged borrower
  loses its income, misses 3 days, defaults; bank seizes cash + wheat at
  market valuation, fire-sells it, writes off the shortfall — full
  ticks, every-tick invariants, books reconciling. Natural defaults
  also occur unstaged: seeds 42 and 7 each produce 6 organic defaults
  by year 4 (loans first issue ~day 112, 2–3 concurrent).
- **`probe_rate_shock` ✓** (pinned seed 42): control run borrows
  organically after tick 100; the same world with `SetBankRate` 150%
  at tick 100 borrows strictly less. Calibrated once, frozen.

### Verification

- `npm run check` exit 0 (121 sim-core unit tests; 7 integration/
  determinism/probe suites; 7 persistence; tsc; vitest). `check:full`
  exit 0 incl. `soak_1500`.
- Soak matrix (release, 3650 ticks): seeds 42/7/123/6 all at
  **13 employed, hunger 16–19** — unchanged from the contract-kernel
  baseline; the bank adds credit without destabilizing the design
  center.
- **Known issue (recorded, deferred)**: the pop-100 DECADE horizon
  drifted between dystopia variants (13 → 7 employed at year 10; food
  production dead by day 600 in both). Its formal acceptance — the
  1-year `scale.rs` run with every-tick invariants — stays green.
  Decade-horizon pop-100 health is Phase 4 territory (`soak_10y`,
  welfare/policy levers) per PLAN; do not chase it piecemeal.
- Saves break again (SimState/Books/TxKind/Event/PlayerCommand grew);
  schema_version 1, no released saves. Metrics CSV gained bank_cash /
  debt_outstanding / loans_active / loan_defaults columns.

### Exact next task (Phase 3 closes next)

1. Session protocol: `npm run check` first.
2. Remaining before Phase 3 can be declared complete (PLAN.md): the
   **contract view UI** (terms, parties, payment/delivery history,
   breaches, penalties — BRIEF's v1.0 screen; the negotiation history
   table can start as the SupplyReview/BorrowReview decision trail) and
   **negotiation v1's offer/counteroffer log** (BRIEF: "log every
   negotiation completely" — grow it on supply-contract formation, then
   surface it in the view). Suggested one increment: sim-side
   negotiation log + snapshot/detail plumbing + the contract view panel,
   launch-verified.
3. Then declare Phase 3 complete (check:full + PLAN/PROGRESS updates)
   and open Phase 4 (government, events, emergence probes, soak_10y —
   which also owns the recorded pop-100 decade issue).
4. Soak checkpoints unchanged: seeds 42/7/123/6 at 365/1500/3650 +
   pop-100; `sim-cli metrics --csv` on surprises.

---

## Session 4 — 2026-07-26 — Phase 3 increment 1: the contract kernel

### Supply contracts v1 (requirements form) — DECISIONS #026

- `contracts.rs`: the `Contract` entity (hashed state, binds businesses,
  survives takeovers) with the full lifecycle — Active → Completed /
  Breached (3 consecutive misses) / Terminated (voluntary underwater
  exit). Terms lock a unit PRICE (posted − 5% commitment discount, gated
  by the buyer's input reservation cap) and a DAILY CEILING; each day
  the buyer takes its current need up to the ceiling. Settlement runs in
  the reserved tick phase 6: goods zero-sum, cash through the ledger
  (`ContractDelivery`/`ContractPenalty`), misses penalized 25%
  cash-capped, books categorized at the site
  (`penalties_received/paid` join the cash identity).
- Formation is a utility-engine decision (`SupplyReview`: Sign vs
  StaySpot — greed weighs the discount, caution buys supply security,
  gamblers hold out until cover thins) on the buyer's weekly stagger;
  underwater buyers walk away past an honesty-widened tolerance
  (`ContractExit`, exit penalty). Contract performance now drives
  relationships (commercial reliability) and reputation ("unreliable"
  beliefs from misses/walk-aways) — the BRIEF's contract-performance
  channel, first of the Phase 3 reputation drivers.
- Interlocks: sellers produce toward commitments, withhold them from
  market offers, and read glut/tool gates on free stock; buyers protect
  due payments in their market budget; the takeover demand gate counts
  committed flow. New invariant `contract_reconciliation` (schedule
  alignment, counter bounds, `paid == unit_price × delivered_units`
  exact, penalty ceilings) runs every sweep.
- **Five engineered collapses on the way** — each diagnosed from
  `sim-cli metrics` CSVs vs a pre-contract baseline worktree, each fixed
  as a recorded mechanic (full narrative in DECISIONS #026): weekly
  lumps (→ daily cadence), the committed-seller stockout ratchet
  (→ total-stock stockout marks), the fixed-quantity EMA anchor
  (→ requirements form + the demand-pull channel: stockout days add
  one-for-one to planned production/input orders), the latent
  dead-business wage ratchet to the $10k ceiling (→ raises need strictly
  positive profit; unfundable offers walk down), and the staffed-zombie
  deadlock (→ working-capital owner injection; second slice of
  "invest").
- **Recorded scope tradeoff**: v1 contracts cover durable industrial
  inputs only (`contracts::contractable`). Food-chain (wheat/flour)
  contracts collapsed every 10-year pop-29 soak — households are
  price-takers for survival food, so no reservation cap disciplines the
  chain and distortions land in its razor-thin cash margins. They return
  with the bank's working-capital credit. Flagged, not silent.

### Verification

- `npm run check`: **exit 0** (fmt, clippy -D warnings, 114 sim-core
  unit + 5 integration suites + 4+1 determinism + 7 persistence, tsc,
  vitest 11/11). `check:full` green — see below.
- Phase 3 acceptance criterion 1 **green**: integration test
  `supply_contract_negotiated_and_fulfilled_end_to_end` (seed 42, 400
  days, unstaged) — signed (traceable to its journaled SupplyReview),
  delivered, ≥ 1 contract Completed with the paid-per-unit identity
  exact and conservation green.
- Soak matrix (release, 3650 ticks): seeds 42/7/123/6 all land at
  **13 employed, hunger 12–21** — on par with the strongest pre-contract
  matrix (#022: 13 employed) across more seeds; pop-100 decade ends
  13 employed / 83 hungry (was ≈94 hungry). The two general fixes (wage
  ratchet, working-capital injection) are net stabilizers beyond
  contracts.
- Saves break (state shape + calibration); schema_version stays 1
  pre-1.0, no released saves.

### Exact next task (Phase 3 continues)

1. Session protocol: `npm run check` first.
2. Next increment options, in PLAN order: **the bank** (deposits, credit
   assessment, loans, collateral, defaults, foreclosure, liquidity/
   solvency — tick phase 7 is reserved; `default→foreclosure` flow test
   and `probe_rate_shock` are the remaining Phase 3 acceptance
   criteria), or first grow **negotiation v1** (offer/counteroffer log
   on supply contracts — BRIEF requires complete negotiation logging;
   the contract view UI wants it). Suggested: the bank next — it
   unblocks food-chain contracts (working-capital credit), employment
   contracts, and both remaining acceptance tests; negotiation logging
   can ride the same increment as the contract view.
3. Soak checkpoints after every economy change: seeds 42/7/123 (+6 — it
   earned its place) at 365/1500/3650 plus `--population 100`; on
   surprises dump `sim-cli metrics <save> --csv`.

---

## Session 3 — 2026-07-26 — Phase 2: COMPLETE

### Agent inspector (the final acceptance item)

- `inspect.rs`: `AgentDetail::capture` — identity, all nine traits,
  memories/relations/beliefs rendered with names, and the agent's last
  ten `DecisionRecord`s with `explanation()` verbatim. Served over the
  on-demand detail protocol reserved in ARCHITECTURE.md: a
  `get_agent_detail(id)` Tauri command through the sim thread's
  request/reply channel — the 10 Hz snapshot stays lean.
- UI: agent rows are clickable; the Agents panel swaps to the inspector
  (1 s refresh while open, back link to the table).
- **Launch-verified**: clicked Falk Voss (greed 94, aggression 95) and
  read four real price-review explanations with full scores — including
  his visible arc from greedy raises while profitable to hard cuts as the
  glut built. Phase 2's acceptance criterion met on screen.
- Phase 2 "done when" scorecard: utility-scoring tests ✓ ·
  `probe_reputation` ✓ · explanation visible in the inspector ✓ ·
  determinism suite ✓. Deferrals recorded in PLAN/DECISIONS #023–#025
  (driver-gated reputation dimensions; action-space entries needing
  Phase 3+ systems; wage/dividend reviews migrate to the engine
  opportunistically).

### Exact next task (Phase 3 start — contracts and finance)

1. Read PLAN.md Phase 3 and BRIEF.md's contracts/bank sections first.
   Session protocol: `npm run check` before building.
2. Suggested first increment: the contract kernel — a `Contract` entity
   (parties, terms, schedule, state machine) with ONE type end to end
   (the supply contract: recurring good delivery at an agreed price),
   deterministic fulfillment in the settlement phase (tick phase 6,
   reserved), breach detection with penalties through the ledger, and a
   `contract_reconciliation` invariant. Negotiation can start as
   take-it-or-leave-it posted terms and grow the offer/counteroffer log
   next.
3. Soak checkpoints after every economy change: seeds 42/7/123 at
   365/1500/3650 plus `--population 100`; on surprises dump
   `sim-cli metrics <save> --csv`.

---

## Session 3a — 2026-07-26 — Phase 2: memory, relationships, reputation (probe passing)

### Reputation v1 (DECISIONS #025) — `probe_reputation` PASSES

- `reputation.rs`: per-agent BELIEFS about others (cap 16, hashed state,
  strangers neutral, weekly drift) — reputation propagates, it isn't a
  global score. Live dimensions: reliable (payroll observed/missed),
  generous (wage moves), ruthless (firings); the rest arrive with their
  Phase 3/4 drivers. Rumor channel: weekly workplace + neighborhood
  gossip (listener moves ¼-gap per subject; **neutrality is silence** —
  only intensity ≥ 8 beliefs get spoken, so ignorant consensus can't
  erase firsthand knowledge). Consumer: non-desperate seekers refuse
  owners believed unreliable (< 26).
- `probe_reputation` (pinned seed 42, trajectory-latched): a
  machinery-produced payroll failure → firsthand victim beliefs → the
  news reaches a non-witness through gossip. Probe design lessons
  recorded in #025: victims need a venue (hence the neighborhood
  channel), and opinions legitimately fade/compete (hence latching, not
  end-state persistence).
- Soak matrix unchanged from the relationships run — reputation bites
  only after public failures. +4 unit tests + the probe.

### Relationships v1 (DECISIONS #024)

- `relationships.rs`: all seven private dimensions (trust, affection,
  fear, respect, resentment, dependence, commercial reliability), sparse
  per-agent maps (cap 16, most-neutral eviction, strangers implicitly
  neutral), bounded-step updates at every existing interaction site
  (payroll paid/unpaid, hire, fire, tenure week, wage moves, takeover
  deals, leaving a job), weekly drift toward neutral in phase 10.
- Consumer: the switch premium is bond-adjusted (±5% max, 2% floor) —
  attachment binds, resentment repels; neutral relations reproduce prior
  behavior exactly. Test: identical wages and loyalty, only the private
  bond differs — the stranger takes the raise, the bonded worker stays.
- Matrix: seeds 7/123 and the 100-town hold their envelopes; knife-edge
  seed 42 lands on a harsher branch this run (10 employed, food ~$21.67 —
  the same alive-and-trading family it has oscillated within; per-seed
  ending selection is explicitly not a tuning target). +5 tests.

### Memory v1 (DECISIONS #023)

- `memory.rs`: bounded per-agent store (12), hashed state, formed at
  event sites only (UnpaidBy 90 / FiredBy 70), reinforcement instead of
  duplication, 2 milli-confidence decay per day in the newly-activated
  tick phase 10, weakest-first eviction. Consumer: a non-desperate agent
  refuses to work for a business they hold an active grievance (strength
  ≥ 20) against — matching and switch targeting — until decay or
  desperation. End-to-end test: a payroll-failing bakery, solvent again,
  stays shunned until one ex-worker goes broke and another forgets.
- Full soak matrix unchanged to the cent (grievances rarely decisive in
  healthy runs — by design); +4 tests.

### Exact next task (Phase 2 completion)

1. The **agent inspector** — the last open Phase 2 acceptance item ("a
   real decision's explanation visible in the inspector"). Implement the
   on-demand detail-query protocol reserved in ARCHITECTURE.md (inspector
   fetches by entity id — never fatten the 10 Hz snapshot): a
   `get_agent_detail(id)` Tauri command returning identity, traits, cash/
   home/pantry, memories (rendered), relations and beliefs (labeled), and
   the agent's recent DecisionRecords with `explanation()` verbatim.
   UI: click an agent row → detail panel. Launch-verify by reading a real
   explanation in the running app.
2. Then declare Phase 2's acceptance state in PLAN/PROGRESS (utility
   tests ✓, probe_reputation ✓, inspector explanation ✓, determinism ✓)
   and assess remaining Phase 2 scope (complete action set, remaining
   owner reviews on the engine) before moving to Phase 3.
3. Soak checkpoints after every economy change: seeds 42/7/123 at
   365/1500/3650 plus `--population 100`; on surprises dump
   `sim-cli metrics <save> --csv`.

---

## Session 2b — 2026-07-19 — Phase 2 begun: decision engine v1 (traits, scored price review, labor mobility, entry/exit)

### What was built

- **Traits** (AGENT_DESIGN.md): nine personality dimensions per agent,
  integer 0–100, rolled from a dedicated per-agent `"traits"` substream in
  fixed field order (adding features elsewhere never reshuffles who
  someone is).
- **Decision engine core** (`decision.rs`, DECISIONS #019): utility scores
  (the one sanctioned float zone) over {raise, cut hard, cut, hold};
  enum-order tie-breaks encode the old cascade priority; neutral traits
  reproduce the Phase 1 rule family exactly. Greed and aggression act in
  **narrow threshold bands** (raise ±0.4 stockout-days; idle-cut between
  42–58% utilization) — traits decide ambiguous calls, never clear ones.
- **The price review runs through the engine**; every review journals a
  `DecisionRecord` (inputs, all scores, choice) that renders its own
  explanation for the future agent inspector. Records are outputs: saved,
  never hashed, never read back; ring-capped at 10k.
- Twin-run determinism now asserts decision-sequence equality.

### Verification (all run this session)

- All suites green: 71 sim-core unit (5 engine + traits tests new) +
  determinism + industry/construction/scale integration + persistence;
  `npm run check` and `check:full` exit 0.
- **Seeds are economically distinct for the first time** (RNG previously
  only named agents). Three-seed soaks (42/7/123) all hold the year-10
  food core (13 employed each) with genuinely different histories — food
  $6.49 / $6.92 / $4.56, different year-1 staffing paths.
- Calibration lesson recorded in #019: wide trait bands (idle-cut at ±20
  points of utilization) let timid owners cut at healthy mill utilization
  and deflated two of three towns to collapse; the shipped bands are
  deliberately narrow.

### Labor mobility (second engine increment, this session — DECISIONS #020)

- Weekly per-agent job reviews: employed workers switch to open jobs that
  clear a loyalty-widened 10–20% premium (`JobSwitched` events + records);
  the unemployed hold out above an ambition-scaled reservation wage that
  **decays to zero over a patience-scaled 30–90-day horizon** and collapses
  under desperation; matching honors reservations. +5 unit tests.
- Soaks: seeds 7/123 hold their year-ten cores (13 employed); the 100-town
  matches its pre-mobility decade shape. **Seed 42 got harsher**: workers
  now flee its wage-cutting farm mid-trough and it dies ~year 4 where
  captive labor once kept it alive (year 10: 5 employed). Labor flight
  from failing firms is real economics; the counterweight is entry/exit,
  which is the next increment — re-evaluate all seeds after it lands.

### Entry/exit v1 (third engine increment, this session — DECISIONS #021)

- Takeover-revival of moribund businesses: wealthy entrepreneurs
  (ambition + risk tolerance > 120) buy dead firms at asset value through
  the ledger (`BusinessSale`/`BusinessSold`/`Takeover` record), quit their
  jobs, and the injection machinery restarts the firm the same tick; the
  broke seller is paid and becomes a job seeker. Demand-gated after two
  recorded failures (zombie entrepreneurship with no gate; blocked
  revivals with a too-strict shortage gate).
- **Best small-town matrix yet: seeds 42/7/123 all hold 13-employed
  year-ten cores** — seed 42's mobility-induced farm death is cured by
  revival restoring competition. All 29-town flagged limitations now have
  their counterweight mechanic in place.
- **Open item (flagged)**: the 100-town regresses at decade scale
  (≈16 → ≈6 employed by year 10) under takeover churn — chronic shortage
  keeps the gate open and frequent ownership rotation destabilizes
  production. Year-one scale acceptance is unaffected (scale.rs green).

### The zero-revenue price deadlock (fourth increment, this session — DECISIONS #022)

- Ownership telemetry (new `BizDay` owner/wage columns in the metrics CSV)
  overturned the churn hypothesis: only ~20 sales a decade at 100 scale.
  The real disease: after the year-one price spike, businesses froze at
  unaffordable prices with zero sales forever — every corrective signal
  structurally silent (stockout needs sales, glut needs stock, idle cut is
  profit-gated). A mill held flour at $42.45 for six years.
- Fix: `dry_windows` on Business + a deadlock breaker in the heavy-cut
  score — three consecutive zero-revenue windows with stock or staff force
  the cut regardless of profitability. Run length is load-bearing: firing
  on one quiet week turned duopoly alternation into town-razing price
  wars (recorded in #022).
- **Best full matrix to date**: seed 42 at its healthiest ending ever
  (13 employed / 13 hungry / food $3.93); seeds 7/123 hold; the 100-town
  un-freezes to ~13–15 employed across the decade with food repricing
  $23 → $9.73. The 100-town's ~94 hungry is Phase 4's welfare problem by
  design.


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
