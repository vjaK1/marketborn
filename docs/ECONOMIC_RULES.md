# Economic Rules

Binding specification of the simulation's economic mechanics. The tick phase
order and every cadence here are **part of the determinism contract** — change
them only with a DECISIONS.md entry and a save `schema_version` review.

Status: Phase 3 in progress (contract kernel live; bank next). Sections
marked *[activates: Phase N]* are reserved slots in the phase order,
documented now so later systems slot in without reordering anything.

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
| 3 | **Production** | Businesses run recipes; equipped workers add the tool bonus and wear tools down (see §Production). |
| 4 | **Labor market** | Job matching, then daily payroll (see §Labor). |
| 5 | **Goods markets** | Posted-price clearing per good, in `Good::ALL` order: wheat → flour → food → iron ore → steel → tools → wood → bricks → home (see §Markets). |
| 6 | **Contract settlement** | Every active supply contract due today settles in contract-id order: the buyer takes its current input need up to the contracted daily ceiling at the locked price (goods seller→buyer zero-sum, cash through the ledger); shortfalls are penalized misses; three consecutive misses breach (see §Contracts). |
| 7 | Banking | *[activates: Phase 3, bank increment]* |
| 8 | **Consumption** | Each agent eats 1 food or goes hungry; the wealthy take a second, comfort meal; then perishable stocks spoil (see §Consumption). |
| 9 | **Agent decisions** | Phase 0: business owner decisions — emergency staffing daily; price/wage/dividend review weekly (see §Decisions). |
| 10 | **Memory, relationships & reputation** | Every memory fades a little (`memory::decay`); forgotten memories drop. On the agent's weekly stagger day: workplace + neighborhood gossip (listener moves ¼ of the gap toward each speaker per subject; neutrality is silence — only intensity ≥ 8 beliefs get spoken), then relations and beliefs drift one step toward neutral; fully-neutral entries drop. Formation/updates happen at event sites, never by reading the journal back. |
| 11 | **Bookkeeping** | Sales EMAs update; metrics captured; invariants checked (every tick in debug, on the hash cadence in release); state hash appended to the manifest on the cadence. |

## Cadences

| What | Cadence |
|------|---------|
| Wages | every tick (daily) |
| Contract deliveries | every tick (daily) — the town runs on a one-day cadence; weekly lumps starved it (DECISIONS.md #026) |
| Contract formation & underwater review | on the buyer business's weekly review stagger |
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

1. **Offers**: every business with `sells == good` offers its **free
   stock** — on hand minus its daily contract commitments
   (`contracts::free_stock`; promised goods are not for sale) — at its
   posted price. Offers sort by `(unit price, seller id)`.
2. **Orders** (buyers), sorted by `(urgency, account)` where accounts order
   businesses before households, then by id (DECISIONS.md #008):
   - *Producers* buy inputs up to `INPUT_TARGET_DAYS (3) × daily input need`,
     where daily need derives from the **planned** rate (sales EMA plus
     recent stockout days — the demand-pull channel, §Production). Urgency
     0 when holding less than one day of input, else 1. Spending is capped
     to keep a 3-day payroll reserve **plus any contract payment due
     today**. Producers carry a **reservation price**: they refuse
     unit prices above `70% × (output price × output per batch) ÷ input units
     per batch` — the marginal-revenue cap that damps cost-push spirals
     (DECISIONS.md #011).
   - *Tool users* order up to one tool per current worker. Gates: no order
     while sitting on unsold output (stock of the sold good above the
     light-glut threshold — no capital spending while glutted), and none
     when the bonus rounds to zero batches. Urgency is
     always 1 (efficiency good, never survival). Willingness to pay:
     `TOOL_REVENUE_SHARE_BP (9000) × bonus output per equipped worker-day ×
     output price × TOOL_LIFE_WORKER_DAYS` — the capital-goods analogue of
     the input reservation price. Same payroll-reserve budget as inputs.
   - *Households* buy food up to `PANTRY_TARGET (3) + meals − pantry`,
     where meals = 2 above the comfort floor (§Consumption), else 1.
     Urgency 0 when the pantry is empty, else 1. Households are
     price-takers (survival good); their cash is the only limit.
   - *Homes*: a household holding cash ≥ `HOME_CASH_FLOOR ($600.00)` that
     does not yet own a home orders exactly one, paying at most half its
     cash. Ownership is permanent; owned homes remain in the
     goods-conservation totals (DECISIONS.md #017).
3. **Execution**: each order takes the cheapest offer (ties → lower seller
   id), limited by remaining demand, offer quantity, the buyer's reservation
   price, and affordable units at the buyer's live balance/budget; then moves
   to the next offer. Money moves buyer → seller through the ledger; goods
   move seller → buyer.
4. **Signals**: a seller gets a stockout day only when it **sold out** —
   it moved units today, holds zero **total** stock, and aggregate demand
   went unmet. A business with nothing to sell all day isn't participating
   and gets no scarcity signal (dead businesses must not ratchet prices),
   and a seller holding only contract-committed stock has no scarcity
   problem — that stock is sold either way, and counting the residual spot
   shortfall was a one-way price ratchet (DECISIONS.md #026).
   Volume-weighted average execution prices are recorded per good
   (`market.last_prices`, metrics, chart); contract deliveries settle
   off-market and do not move them.

## Contracts (Phase 3 — supply contracts, requirements form)

One type live (DECISIONS.md #026): a buyer business locks a recipe
input's PRICE and a DAILY CEILING with one seller; each day the buyer
takes its **current** input need up to the ceiling at the locked price.
Price rigid, quantity adaptive — both opposite rigidities (weekly lumps,
fixed quantities) collapsed towns and are documented in #026.

- **Scope**: durable industrial inputs only (`contracts::contractable` —
  iron ore, steel, tools, wood, bricks). The survival-food chain (wheat,
  flour) stays spot in v1: its downstream buyers are price-taking
  households, so no reservation cap disciplines a locked price; returns
  with the bank.
- **Formation** (weekly, buyer's review stagger, before owner reviews):
  for each recipe input without an active contract, the buyer's owner
  scores **Sign vs StaySpot** through the utility engine — the 5%
  commitment discount weighed by greed, supply security (input cover vs
  the 3-day target) weighed by caution, a commitment cost that scales
  with risk tolerance squared (gamblers hold out until cover thins).
  Both outcomes journal a `SupplyReview` decision record. Terms:
  cheapest capable seller (price, then id), unit price = posted − 5%
  (floor 1¢), must clear the buyer's input reservation cap; ceiling =
  buyer's daily need capped at the seller's contractable share (80% of
  bare-handed capacity, floored at one unit) net of prior commitments;
  buyer must afford one full take beyond its protected reserves. The
  buyer refuses sellers whose owner it publicly believes unreliable
  (< 26, as hiring). Seller side is v1 take-it-or-leave-it (posted terms
  are their offer); the offer/counteroffer log is the next increment.
- **Settlement** (phase 6, contract-id order): take = min(ceiling,
  buyer's `daily_input_need`). Zero-need days settle trivially. Success
  moves goods (zero-sum) and cash (`TxKind::ContractDelivery`; books:
  seller revenue, buyer input/tool costs; seller `sold_today` includes
  contract volume so the EMA sees it). Failure is a **miss**: the
  failing side (seller when short of goods — checked first — else the
  buyer short of cash) pays 25% of the missed take's value, cash-capped
  (`TxKind::ContractPenalty`, books `penalties_paid/received`).
  `consecutive_misses ≥ 3` ⇒ **Breached** (terminal). Schedule: 84 daily
  deliveries ≈ one quarter, `next_due` advances every due date hit or
  missed; exhaustion ⇒ **Completed**. Expiry is v1's renegotiation:
  parties re-sign at fresh prices.
- **Underwater exit** (weekly, same review): a buyer whose locked price
  exceeds its current input reservation cap beyond an honesty-widened
  tolerance (0–10% past the cap) walks away, paying one exit penalty
  (25% of a full-ceiling delivery, cash-capped) ⇒ **Terminated**;
  journaled as a `ContractExit` decision. This is the escape valve that
  lets crisis-priced contracts die instead of their buyers (#026).
- **Interlocks**: committed ceilings are withheld from the seller's
  market offers daily (promised goods are not for sale), added to the
  seller's production target, and netted out of its glut signal and
  tool-purchase gate (committed stock is sold stock in waiting — but
  stockout marks still require zero TOTAL stock, see §Markets). Buyers
  protect today's takes in their market budget. The takeover-revival
  demand gate counts town-wide committed flow as live demand.
- **Social fabric**: each clean delivery nudges both owners' mutual
  commercial reliability (+2) and trust (+1); a miss or walk-away hits
  the victim's view of the failer (reliability −8, trust −4, resentment
  +4) and seeds a public "unreliable" belief (−15) that gossip then
  carries — contract performance is a reputation driver (BRIEF).
- **Bookkeeping invariant** (`contract_reconciliation`, every sweep):
  parties exist; terms sane; counters bounded; Active schedules aligned
  (`next_due == start + (settled+1) × every`); terminal states
  consistent; `paid_total == unit_price × delivered_units` exactly;
  penalties within `penalty events × full ceiling penalty`.

## Production

Per business, in id order: `desired = planned_daily_sales × (OUTPUT_TARGET_DAYS
(4) + 1) + contract commitments − current stock`; batches = min(ceil(desired /
output-per-batch), capacity batches, input-limited batches); consume inputs,
add output — all creation/destruction through the goods ledger.
`planned_daily_sales = expected_daily_sales + stockout_days`: recent
stockout days add to the plan one for one (bounded — the counter resets
each review window). This **demand-pull channel** lets shortages propagate
upstream as quantities, not only prices; without it a contract-fed chain
locks at the contracted flow forever (DECISIONS.md #026). The same planned
rate drives input orders (§Markets). `expected_daily_sales = ceil(sales
EMA)`, minimum 1 so a stalled business still plans a minimal batch; sellers
add their committed contract ceilings on top so deliveries are on hand at
settlement.

**Tools (capital good).** Tool-using businesses (farms, mine) equip
`min(workers, tool stock)` workers. Capacity = `workers × batches-per-worker
+ equipped × batches-per-worker × TOOL_BONUS_BP (5000) / 10000` — the
fractional-batch remainder rounds toward zero at the business level and is
not produced (no party receives it). On any day the business produces,
each equipped worker adds one worker-day of tool wear; every
`TOOL_LIFE_WORKER_DAYS (6)` of wear destroys one tool (burned through the
goods ledger; breakage is capped by tools on hand, leftover wear carries
over). Idle days cause no wear.

Sales EMA: integer milli-units, `ema += (today·1000 − ema) / 8`, toward zero.

## Labor

- **Job reviews** (weekly, per-agent stagger `(tick + agent id) % 7`, in
  id order, executed immediately — DECISIONS.md #020): an employed worker
  switches to the best open job (highest wage, tie → lower business id,
  hiring cash gate applied) whose wage clears a loyalty-widened premium of
  10–20% over the current wage, adjusted by the private bond toward the
  current owner (trust + affection + dependence − resentment, ±5% max,
  2% floor — attachment binds, resentment repels; DECISIONS.md #024)
  (`JobSwitched` event + decision record). Tenure drips affection and
  dependence weekly; the seven relationship dimensions update in bounded
  steps at every payroll, hire, fire, wage move, and takeover (#024). An
  unemployed agent holds out above a reservation wage — 0.5–1.5× the going
  food price by ambition, decaying linearly to zero over a
  patience-scaled 30–90-day unemployment horizon, zero at once when
  desperate (hungry, or savings below 30 days of food). Holding out
  against a live offer journals a decision record.
- **Matching** (daily): businesses in id order fill vacancies from
  *willing* job seekers in id order (owners never seek; seekers decline
  wages below their reservation, and a non-desperate seeker refuses a
  business they hold an active grievance against — being stiffed,
  importance 90, or fired, importance 70 — until it decays below strength
  20 or desperation overrides pride, and likewise refuses an owner they
  publicly believe unreliable, reliable < 26; DECISIONS.md #023/#025).
  Marginal hiring
  gate: a business staffs up only as far as cash covers
  `HIRING_CASH_DAYS (5) × wage` per resulting worker — so a downsized
  business can bootstrap back one hire at a time instead of needing the
  full target payroll upfront.
- **Payroll** (daily): each worker is paid the posted daily wage in hire
  order. Workers who cannot be paid quit immediately (`QuitUnpaid`), and the
  business logs a public `MissedPayroll` event.
- **Vacancy aging**: unfilled vacancies age daily; read by the wage review.

## Consumption

Each agent, in id order, eats 1 food from the pantry or increments a hunger
streak (`AgentHungry` events at streak 1, 7, 14, …). An agent holding cash ≥
`COMFORT_CASH_FLOOR ($400.00)` takes a **second, comfort meal** — but only
when a meal would still remain afterward, so comfort never causes hunger.
This is the channel that returns idle household savings to circulation: in a
closed loop where consumption is otherwise fixed at 1 food/day, every wage
surplus accumulates forever and aggregate demand decays into a deflationary
collapse (DECISIONS.md #014). There is still no mortality or welfare;
structural unemployment produces visible hunger — by design.

**Spoilage** (end of the consumption phase). Every holder loses
`spoilage_bp` of each perishable stock per day — food: 400 bp (4%) — with
the per-holder amount rounding toward zero; the sub-unit remainder
**explicitly stays fresh** (small stocks like pantries never rot). Burns go
through the goods ledger (conservation targets stay exact) and are recorded
per good in metrics. All other goods are durable. Consequences are priced
into production: perishable output targets `PERISHABLE_TARGET_DAYS (2) + 1`
days instead of the durable `OUTPUT_TARGET_DAYS (4) + 1` — big enough to
bridge the upstream 3-day input-batch supply oscillation, small enough that
daily rot stays a minor cost (DECISIONS.md #015).

## Decisions (business owners + capital decisions)

Weekly, on the agent stagger, before owner reviews (DECISIONS.md #021):

- **Takeover**: a non-owner with ambition + risk tolerance > 120 and
  sufficient cash (price + two hires of restart capital + $100 reserve)
  buys the highest-asset-value moribund business — no staff, and even the
  sitting owner's savings cannot fund one hire — provided standing demand
  for its good is nonzero. Price = cash + inventory at market prices, paid
  seller-to-buyer through the ledger; ownership swaps; the buyer quits any
  job; the seller becomes a job seeker; the ordinary injection/hiring
  machinery restarts the firm the same tick.

Daily:

- **Owner capital injection** — the brief's "invest" action, two
  triggers: the business cannot fund one hire (`cash < 5 × wage`), or it
  is **staffed with a recipe to run and cannot afford one day of inputs**
  after its protected reserves (the working-capital slice — without it a
  firm hovering just above the hire floor paid wages forever while
  producing nothing, and its rich owner never stepped in; DECISIONS.md
  #011/#026). The owner transfers savings above a $100.00 personal
  reserve, topping the business up to two hires of runway plus a week of
  inputs at last observed prices.
- **Emergency downsizing**: if cash < 2 days of payroll, the most recently
  hired worker is let go (one per day).

On review day (`(tick + id) % 7 == 0`):

- **Price** for the sold good (Phase 2: utility-scored through the
  decision engine — DECISIONS.md #019): the owner scores
  {raise 7%, cut 5%, cut 2%, hold} and takes the maximum (ties break in
  that order). Neutral traits reproduce the rule family: raise on ≥ 2
  stockout days; heavy cut above 8 days of stock; light cut above 6 days
  (strictly above the 5-day buffer — #015) or, from a profitable window,
  below ~50% bare-handed utilization (the anti-monopolist corrective —
  #014; tool bonus excluded). Greed shifts the raise threshold ±0.4 days
  and weights it; aggression shifts the idle-cut threshold between 42% and
  58% utilization — narrow bands by design (#019). **Deadlock breaker**
  (#022): three consecutive zero-revenue windows while holding stock or
  staffing capacity force the heavy cut regardless of profitability (a
  price earning nothing cannot lose by falling; one dry week is normal
  duopoly alternation and is ignored). Every review journals a
  `DecisionRecord` with all scores and inputs. Steps stay integer with
  explicit floors/ceilings (10¢ / $100,000). Window counters reset;
  `dry_windows` extends on a zero-revenue window and resets on any sale.
- **Wage**: vacancies unfilled ≥ 7 days **and** a strictly positive window
  profit → raise 5% (a loss-making business bidding wages up while broke
  is the death-spiral input; and `≥ 0` was a latent trap — a dead
  business's zero-profit window ratcheted its wage to the ceiling, pricing
  rehiring and takeover revival out of existence, DECISIONS.md #026).
  Fully staffed and the 7-day window ran a loss → cut 3%. Vacancies open
  but **cash below one hire at the posted wage** → cut 3% (an offer the
  till cannot fund is walked down until it becomes hirable-into again).
  Floor $3.00, ceiling $10,000/day.
- **Dividend**: cash above `21 days of payroll at target headcount + 7 days
  of input purchases at last observed prices` pays 25% of the excess to the
  owner. The buffer uses *target* headcount so owners cannot strip a
  downsized business of its restaffing capital.
- Window profit (revenue − costs over the window) is recorded for the UI and
  the wage rule.

All parameters above are initial calibrations; they are expected to be tuned
in later phases, but changes must be recorded (they shift every hash).

## World parameters (Phase 1 calibration)

| Entity | Values |
|--------|--------|
| Population | 29 (10 owners, 19 staffed jobs, 0 unemployed at start) — configurable |
| Business scaling | one instance per N agents (ceil, min 1): farms /15 · mills /35 · bakeries /30 · each specialist shop /100 — 29 agents ⇒ 10 businesses, 100 ⇒ exactly 20 (DECISIONS.md #018) |
| Agent start | $300.00 cash, pantry 3, no home |
| Food chain | 2 farms (3 workers, wage $7.00, wheat $5.50, uses tools) · mill (3 workers, $7.00, flour $7.60) · bakery (4 workers, $7.00, food $5.40) |
| Industry chain | mine (1 worker, wage $6.00, iron ore $7.50, uses tools) · steelworks (1, $6.00, steel $15.00) · tool factory (1, $6.00, tools $22.00) |
| Construction chain | lumber camp (1 worker, $6.00, wood $5.00, uses tools) · brickworks (1, $6.00, bricks $6.00, uses tools) · construction co (1, $6.00, home $300.00) |
| Recipes | farm → 1 wheat, 2 batches/worker · mill 1 wheat → 1 flour, 6/worker · bakery 1 flour → 2 food, 4/worker · mine → 1 ore, 1/worker · steelworks 1 ore → 1 steel, 1/worker · factory 1 steel → 1 tool, 1/worker · lumber camp → 1 wood, 2/worker · brickworks → 1 bricks, 2/worker · construction 6 wood + 6 bricks → 1 home, 1/worker |
| Business start cash | $1,200.00 each |
| Tools | +50% batches per equipped worker · life 6 worker-days · buyer cap 90% of marginal product |
| Comfort floor | $400.00 cash → second daily meal |
| Home floor | $600.00 cash → buy one home, paying ≤ half of cash |
| Consumption | 1 food per agent per day (2 above the comfort floor) |
| Spoilage | food 4%/day per holder, toward zero (remainder stays fresh); all other goods durable |
| Output buffers | durable 4+1 days · perishable 2+1 days · light-glut signal strictly above at >6 days |

Calibration rationale (every line audited against the closed loop; see
DECISIONS.md #013/#014 for the failure modes that shaped it):

- **Wage bill vs spending pool.** Full staffing costs 13×$7 + 3×$6 =
  $109/day; base food demand is 26 × $5.40 ≈ $140/day. The wage bill must
  sit inside the spending pool or someone runs structural losses from day
  one. Comfort meals make the pool elastic upward when money concentrates.
- **Food capacity.** Bare-handed wheat (12/day) runs just under the mill's
  ~13/day need; tooled farms (18/day) create comfortable surplus. Tools are
  load-bearing for prosperity, not for survival.
- **Industry sizing.** Single-worker stages with capacity ≈ demand
  (~1 unit/day each, matching tool wear of ~1/day from ≈6 equipped
  workers). Capacity 2× demand self-gluts: the stage cuts its own price
  below payroll and dies — the mine's original fate.
- **Chain window.** At steady volume every stage clears its wage at start
  prices, and each buyer cap (70% input share; 90% tool share) sits above
  the upstream price with review headroom: ore $7.50 < $10.50, steel
  $15.00 < $15.40, tools $22.00 < ~$25 (at trough wheat) to $29.70.

## Business accounting (lifetime cash-basis books)

Every business carries `Books`: starting cash plus cumulative revenue,
input costs, tool (capital) costs, wages, dividends, owner investment, net
monetary policy, and spoiled units (a physical write-down, outside the cash
identity). Each flow is categorized at its ledger site — sales/purchases in
the goods market, payroll in labor, dividends/investment in decisions,
policy in the money ledger. Books influence no decision in Phase 1; they
exist for statements (P&L, balance sheet, cash flow — derived views in the
snapshot), the market view, and Phase 3 credit scoring. Balance-sheet
inventory is valued at last market execution prices, falling back to the
business's own posted price for its sold good, else zero (derived view
only, never accounting state).

## Conservation invariants (checked continuously)

1. `money_conservation` — Σ cash == expected total.
2. `goods_conservation` — per good: Σ inventories + pantries == expected
   total. Only the goods ledger (production mints; consumption and tool
   wear burn) moves the target; trades are zero-sum. Checked after the
   specific checks below so a negative stock reports precisely.
3. `non_negative_cash` — no account below zero (structurally impossible via
   the ledger; the check catches bypasses).
4. `non_negative_inventory` — business stock and pantries ≥ 0.
5. `employment_reciprocity` — rosters ↔ employer fields agree; nobody is
   employed twice.
6. `business_books` — per business: live cash == the books' implied cash
   (starting + revenue + owner investment + policy − inputs − tools −
   wages − dividends). A flow that bypasses categorization halts the run.

On violation: the simulation halts (`SimStatus::Halted`), and the report
carries tick, invariant, expected vs actual, delta, and the last 50
transactions touching the affected accounts.
