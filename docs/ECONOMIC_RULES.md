# Economic Rules

Binding specification of the simulation's economic mechanics. The tick phase
order and every cadence here are **part of the determinism contract** — change
them only with a DECISIONS.md entry and a save `schema_version` review.

Status: Phase 0 (minimal food chain). Sections marked *[activates: Phase N]*
are reserved slots in the phase order, documented now so later systems slot in
without reordering anything.

## Time

- 1 tick = 1 simulated day. Tick 0 is the freshly generated world; the first
  `tick()` call produces day 1.
- Display calendar: 360-day years, purely cosmetic (DECISIONS.md #006).

## Tick phase order (binding)

Every tick executes exactly this sequence:

| # | Phase | Phase 0 behavior |
|---|-------|------------------|
| 1 | **Apply queued commands** | All pending commands with `tick ≤ now`, in `(tick, seq)` order. Failures become `CommandRejected` events, never halts. |
| 2 | Scheduled events | *[activates: Phase 4]* |
| 3 | **Production** | Businesses run recipes (see §Production). |
| 4 | **Labor market** | Job matching, then daily payroll (see §Labor). |
| 5 | **Goods markets** | Posted-price clearing per good, in `Good::ALL` order: wheat → flour → food (see §Markets). |
| 6 | Contract settlement | *[activates: Phase 3]* |
| 7 | Banking | *[activates: Phase 3]* |
| 8 | **Consumption** | Each agent eats 1 food or goes hungry (see §Consumption). |
| 9 | **Agent decisions** | Phase 0: business owner decisions — emergency staffing daily; price/wage/dividend review weekly (see §Decisions). |
| 10 | Memory & relationships | *[activates: Phase 2]* |
| 11 | **Bookkeeping** | Sales EMAs update; metrics captured; invariants checked (every tick in debug, on the hash cadence in release); state hash appended to the manifest on the cadence. |

## Cadences

| What | Cadence |
|------|---------|
| Wages | every tick (daily) |
| Business review (price, wage, dividend, window profit) | every 7 ticks, staggered: a business reviews when `(tick + id) % 7 == 0` |
| Emergency downsizing check | every tick |
| State hash + manifest entry | every `hash_every` ticks (default 50) and at every save |
| Invariants | every tick (debug builds); hash cadence (release) |
| Autosave | not yet (UI save button + CLI only; cadence autosave arrives with save-slot management, Phase 5) |

## Money

- All amounts are `i64` minor units (cents) — the `Money` newtype. No floating
  point anywhere in accounting.
- Rates are integer basis points; `Money::mul_bp` computes `x·bp/10000` in
  i128, rounding toward zero.
- Every rounding remainder is explicitly assigned: dividend = 25% (2500 bp) of
  excess cash, remainder **stays with the business**; price/wage steps round
  toward zero with a 1¢ minimum step.
- Every money movement goes through the ledger (`transfer` / `mint` / `burn`):
  balance-checked, atomic, journaled. Nothing else touches `cash` fields.
- Money conservation: `Σ balances == expected_total_money`, adjusted **only**
  by the explicit monetary-policy command `AdjustMoneySupply`.

## Markets (posted prices)

Each good has a registry of standing sell offers, rebuilt each tick from
current business inventory (equivalent to refreshing standing offers):

1. **Offers**: every business with `sells == good` and stock > 0 offers its
   whole stock at its posted price. Offers sort by `(unit price, seller id)`.
2. **Orders** (buyers), sorted by `(urgency, account)` where accounts order
   businesses before households, then by id (DECISIONS.md #008):
   - *Producers* buy inputs up to `INPUT_TARGET_DAYS (3) × daily input need`,
     where daily need derives from the sales EMA. Urgency 0 when holding less
     than one day of input, else 1. Spending is capped to keep a 3-day
     payroll reserve. Producers carry a **reservation price**: they refuse
     unit prices above `70% × (output price × output per batch) ÷ input units
     per batch` — the marginal-revenue cap that damps cost-push spirals
     (DECISIONS.md #011).
   - *Households* buy food up to `PANTRY_TARGET (3) + 1 − pantry`. Urgency 0
     when the pantry is empty, else 1. Households are price-takers (survival
     good); their cash is the only limit.
3. **Execution**: each order takes the cheapest offer (ties → lower seller
   id), limited by remaining demand, offer quantity, the buyer's reservation
   price, and affordable units at the buyer's live balance/budget; then moves
   to the next offer. Money moves buyer → seller through the ledger; goods
   move seller → buyer.
4. **Signals**: a seller gets a stockout day only when it **sold out** —
   it moved units today, holds zero, and aggregate demand went unmet. A
   business with nothing to sell all day isn't participating and gets no
   scarcity signal (dead businesses must not ratchet prices). Volume-weighted
   average execution prices are recorded per good (`market.last_prices`,
   metrics, chart).

## Production

Per business, in id order: `desired = expected_daily_sales × (OUTPUT_TARGET_DAYS
(4) + 1) − current stock`; batches = min(ceil(desired / output-per-batch),
workers × batches-per-worker, input-limited batches); consume inputs, add
output. `expected_daily_sales = ceil(sales EMA)`, minimum 1 so a stalled
business still plans a minimal batch.

Sales EMA: integer milli-units, `ema += (today·1000 − ema) / 8`, toward zero.

## Labor

- **Matching** (daily): businesses in id order fill vacancies from job
  seekers in id order (owners never seek). Marginal hiring gate: a business
  staffs up only as far as cash covers `HIRING_CASH_DAYS (5) × wage` per
  resulting worker — so a downsized business can bootstrap back one hire at
  a time instead of needing the full target payroll upfront.
- **Payroll** (daily): each worker is paid the posted daily wage in hire
  order. Workers who cannot be paid quit immediately (`QuitUnpaid`), and the
  business logs a public `MissedPayroll` event.
- **Vacancy aging**: unfilled vacancies age daily; read by the wage review.

## Consumption

Each agent, in id order, eats 1 food from the pantry or increments a hunger
streak (`AgentHungry` events at streak 1, 7, 14, …). Phase 0 has no mortality
or welfare; structural unemployment produces visible hunger — by design.

## Decisions (Phase 0: business owners)

Daily:

- **Owner capital injection**: if the business cannot fund one hire
  (`cash < 5 × wage`) and the owner holds personal cash above a $100.00
  reserve, the owner transfers savings in (up to funding two hires) —
  the Phase 0 slice of the brief's "invest" action and the channel that
  returns household money to production after a bust (DECISIONS.md #011).
- **Emergency downsizing**: if cash < 2 days of payroll, the most recently
  hired worker is let go (one per day).

On review day (`(tick + id) % 7 == 0`):

- **Price** for the sold good: ≥ 2 stockout days in the window → raise by
  7% (700 bp, min 1¢). Stock > 8 days of expected sales → cut 5%; > 5 days →
  cut 2%. Floor 10¢, mechanical ceiling $100,000/unit. Window counters reset.
- **Wage**: vacancies unfilled ≥ 7 days **and** a non-negative window profit
  → raise 5% (a loss-making business bidding wages up while broke is the
  death-spiral input, not competition). Fully staffed and the 7-day window
  ran a loss → cut 3%. Floor $3.00, ceiling $10,000/day.
- **Dividend**: cash above `21 days of payroll at target headcount + 7 days
  of input purchases at last observed prices` pays 25% of the excess to the
  owner. The buffer uses *target* headcount so owners cannot strip a
  downsized business of its restaffing capital.
- Window profit (revenue − costs over the window) is recorded for the UI and
  the wage rule.

All parameters above are initial calibrations; they are expected to be tuned
in later phases, but changes must be recorded (they shift every hash).

## Phase 0 world parameters

| Entity | Values |
|--------|--------|
| Population | 20 (4 owners, 11 staffed jobs, 5 unemployed) — configurable |
| Agent start | $300.00 cash, pantry 3 |
| Businesses | 2 farms (3 workers each), mill (2), bakery (3), $1,200.00 cash each |
| Recipes | farm: → 1 wheat, 2 batches/worker·day · mill: 1 wheat → 1 flour, 6/worker · bakery: 1 flour → 2 food, 4/worker |
| Start prices | wheat $5.50 · flour $7.60 · food $5.40 |
| Start wages | $9.00/day everywhere |
| Consumption | 1 food per agent per day |

Calibration rationale: ~22 food/day capacity against 20/day demand; each chain
stage roughly breaks even at start prices; wage $9 vs food $5.40 leaves
workers a surplus while the 5 structurally unemployed burn savings — hunger
appears after ~2 sim months unless hiring absorbs them. Emergent price
competition between the two identical farms is expected (staggered reviews
mean they never move the same day).

## Conservation invariants (checked continuously)

1. `money_conservation` — Σ cash == expected total.
2. `non_negative_cash` — no account below zero (structurally impossible via
   the ledger; the check catches bypasses).
3. `non_negative_inventory` — business stock and pantries ≥ 0.
4. `employment_reciprocity` — rosters ↔ employer fields agree; nobody is
   employed twice.

Goods conservation (production/consumption ledger reconciliation) arrives
with Phase 1's multi-chain inventory work, as planned in TEST_PLAN.md.

On violation: the simulation halts (`SimStatus::Halted`), and the report
carries tick, invariant, expected vs actual, delta, and the last 50
transactions touching the affected accounts.
