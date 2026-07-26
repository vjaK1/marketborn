# Decisions

Architecture Decision Records. Newest last. The pinned decisions in
`CLAUDE.md` (workspace layout, tick semantics, i64 money, ChaCha8 RNG,
determinism scope, single-threaded core, state-vs-persistence split, UI
boundary, testing transports, ordered iteration, BLAKE3 hashing) are ADR-000
by reference — change any of them only by amending CLAUDE.md **and** adding
an entry here.

---

## 001 — Canonical serialization via postcard

**Context.** State hashing needs canonical bytes; hand-rolled byte writers
are error-prone and drift from the structs.

**Decision.** `postcard` (v1, alloc) serializes `SimState`; BLAKE3 hashes the
bytes. Postcard is deterministic for a fixed type layout, and every state
collection is ordered (`BTreeMap`/`Vec`), so equal states ⇒ equal bytes.
The same blob format stores the world in saves.

**Consequences.** Field order/type changes shift every hash — acceptable
pre-1.0 (noted in PROGRESS.md per CLAUDE.md's save-versioning policy); from
1.0, hash-affecting changes ride save `schema_version` bumps. No reflection
or schema evolution in the blob — `meta` carries versioning.

## 002 — Stateless RNG substreams, salted per (stream, entity, tick)

**Context.** CLAUDE.md pins named ChaCha8 substreams per system/agent so new
features never reshuffle existing randomness. Storing many RNG states in the
world bloats state and hashing.

**Decision.** Substreams are derived at each use:
`ChaCha8Rng::from_seed(BLAKE3(domain ‖ master_seed ‖ len(name) ‖ name ‖
entity ‖ tick))`. No mutable RNG state exists; the master seed (in config)
plus call-site context is the complete RNG state, trivially save/replay-safe.

**Consequences.** Draws within one (stream, entity, tick) are one sequence;
different ticks are independent streams. Worldgen uses tick 0. The length
prefix domain-separates stream names.

## 003 — Hash covers SimState only; inputs & journal excluded

**Context.** The world carries inputs (command log) and outputs (events,
metrics, manifest) alongside authoritative state. Ring-buffer trimming and
log growth must not perturb hashes.

**Decision.** `state_hash = BLAKE3(postcard(SimState))`. InputLog and
Journal are saved but never hashed. Determinism tests compare event
sequences and metrics explicitly, so output divergence is still caught.

**Consequences.** Everything that influences future outcomes must live in
SimState — enforced by review; rolling stats businesses read (EMAs, window
counters) are SimState fields on Business.

## 004 — Phase 0 economy calibration

**Context.** The slice needs a small economy that neither explodes nor
freezes, with visible dynamics (price moves, hiring, hunger risk).

**Decision.** Parameters in ECONOMIC_RULES.md §Phase 0: 20 agents (4 owners,
11 jobs, 5 structurally unemployed), ~10% food capacity surplus, chain
prices near break-even, produce-to-target inventory rule (4 days), input
restocking (3 days), pantry target 3+1, weekly staggered reviews with
bounded price/wage steps, dividends above a 21-day buffer, LIFO emergency
downsizing at <2 days payroll, hiring only above 5 days payroll.

**Consequences.** Expected emergent behavior: farm duopoly price
competition, headcount oscillation around demand, hunger among the
structurally unemployed after savings deplete (~2 sim months) — welfare
arrives with Phase 4 policy. Parameters are calibration, not contract; any
change shifts hashes and is noted in PROGRESS.md.

## 005 — Tauri dev against built assets; no @tauri-apps/cli

**Context.** `tauri dev` (HMR) needs the tauri CLI npm package and a
devUrl; the packaged flow embeds `frontendDist` at compile time either way.

**Decision.** No devUrl, no tauri CLI dependency. Workflow: `npm run
app:desktop` = vite build + `cargo run -p marketborn`. `npm run app:dev`
serves the UI in a browser (shows a "no backend" shell) for pure-UI
iteration. Frontend must be built before compiling/clippy-ing the shell —
`npm run check` orders it so.

**Consequences.** No frontend HMR inside the shell (acceptable: sim work
dominates); the packaged path (Phase 6) uses the same embedding. Revisit if
UI iteration speed hurts.

## 006 — 360-day display year

**Decision.** Display calendar = 360-day years (12×30), cosmetic only; the
simulation knows nothing but ticks. UI shows `Y{n} · D{n}`.

## 007 — Toolchain & dependency pinning mechanism

**Decision.** `rust-toolchain.toml` pins Rust 1.97.0 (+rustfmt, clippy).
Committed `Cargo.lock` and `package-lock.json` pin the full dependency
graph. `rand_chacha` is additionally source-pinned `=0.3.1` (determinism
contract). Toolchain bumps are deliberate acts recorded here (determinism
scope is per-build anyway).

## 008 — Deterministic buyer ordering: urgency, then businesses, then id

**Context.** The market phase needs a total buyer order. Producers buying
inputs and households buying food could contend for the same good later
(e.g., grain as feed), and Phase 0 already needs a fixed rule.

**Decision.** Orders sort by `(urgency, AccountId)` where `AccountId`'s
derived order puts `Business(_)` before `Agent(_)`, then lower id first.
Urgency 0 = starving household / production-stalled input buyer.

**Consequences.** Within a tier, production supply lines outrank household
convenience purchases; starving households outrank routine restocking. The
enum variant order is load-bearing — documented on the type.

## 009 — Explicit rounding-remainder assignments (Phase 0 instances)

**Decision.** Dividend = 25% (2500 bp) of excess cash rounded toward zero;
remainder stays with the business. Price/wage review steps round toward zero
with a 1¢ minimum step so bounded moves always move. Volume-weighted average
prices (metrics only) round toward zero. Every future division must name its
remainder owner in ECONOMIC_RULES.md.

## 010 — Windows icon generated in-repo

**Context.** tauri-build requires `icons/icon.ico` to embed a Windows
resource; the bundler icon set is a Phase 6 concern.

**Decision.** A minimal 64px programmatically drawn icon (charcoal square,
blue "M" chevron) lives at `src-tauri/icons/icon.ico`, committed. Proper
multi-resolution art ships with Phase 6 packaging.

## 011 — Phase 0 stabilizers: reservation prices, sold-out-only stockouts, marginal hiring, owner injections

**Context.** First 10-year soaks collapsed through two absorbing states:
(a) a dead business (no workers, no stock) kept receiving stockout signals
from unmet demand and ratcheted its price 7%/week to ~$10¹⁵; (b) once every
business fired its staff, all money sat with households and no channel
returned capital to production — permanent town-wide famine while agents
held $10,800.

**Decision.** Four mechanics, all deterministic and documented in
ECONOMIC_RULES.md: (1) stockout days count only when the seller actually
sold units that day (sold out ≠ never had stock); (2) producers refuse input
prices above 70% of the input's marginal revenue — the negative feedback
that stops cost-push spirals; (3) the hiring gate is marginal (5 days of
wage per resulting worker), so downsized businesses bootstrap back; (4)
owners recapitalize a business that can't fund one hire, from savings above
a $100 personal reserve (`TxKind::OwnerInvestment`, `Event::OwnerInvested`).
Supporting rules: dividends buffer against target-headcount payroll so
owners can't strip restaffing capital; wage raises require a non-negative
review window; mechanical price/wage ceilings ($100k/unit, $10k/day) guard
integer sanity.

**Consequences.** Seed 42 now survives 3,650 ticks with the chain staffed
and prices at a stable fixed point (~2× start after early scarcity). The
long-run equilibrium is harsh — one farm died (emergent monopoly) and the
structurally unemployed stay hungry — which is coherent for a world without
welfare (Phase 4), business entry (Phase 2), or population dynamics. The
soak test asserts liveness and invariants, not comfort.

## 012 — Goods ledger with expected-total conservation targets

**Context.** Phase 1 adds the goods-conservation invariant reserved in
TEST_PLAN.md. The money invariant works by comparing actual balances to an
`expected_total_money` that only explicit policy may move; goods needed the
same doorway discipline, but production/consumption volumes are too high to
journal per-unit transactions.

**Decision.** `goods_ledger` is the only path that creates or destroys
goods: `produce` mints into a business inventory, `consume_stock` burns
(recipe inputs, worn-out tools), `consume_pantry` burns household food. Each
adjusts `SimState.expected_total_goods[good]`. Trades move goods between
holders and never touch the targets. The `goods_conservation` invariant
compares per-good world totals (business inventories + pantries) to the
targets every check; it runs after the specific non-negativity checks so a
negative stock reports precisely. No transactions are journaled for pure
goods operations — flows stay visible via `produced_today` and metrics.

**Consequences.** Any inventory mutation outside the doorway (or a trade
that loses a side) halts with a per-good expected/actual/delta report.
Worldgen seeds the targets from generated stock. `expected_total_goods` is
hashed state, so all Phase 0 hashes shift (pre-1.0 policy; no released
saves).

## 013 — Tool economy: +50% bonus, 6-day life, 90% value cap, sized to the demand gap

**Context.** Phase 1's industry chain (mine → steelworks → tool factory)
must be financed entirely by what tools add at the extraction businesses.
Soak-testing exposed hard constraints: (a) the chain's financeable revenue
equals `equipped workers × value share × bonus output × output price` —
tool *life* cancels out of it (longer life = fewer, dearer tools); (b) a
bonus large enough to be valuable can flood the town's fixed demand — at
+100% farm supply gluts, wheat crashes, and the farms the tools equip die;
(c) a stage whose capacity is 2× its steady demand self-gluts, cuts price
below its own payroll, and dies (the mine equipped itself into
overproduction); (d) tool prices above the buyers' value cap freeze demand
through output-price troughs, starving the chain for months.

**Decision.** Tools give +50% batches per equipped worker
(`TOOL_BONUS_BP = 5000`) — sized to close the farms' structural capacity
gap (bare-handed wheat ≈ 12/day vs ~13 needed; tooled ≈ 18) without
flooding it. Life is 6 worker-days, wear only on production days, breakage
through the goods ledger. Buyers pay up to 90% of a tool's lifetime
marginal product (`TOOL_REVENUE_SHARE_BP` — capital, not a per-batch
input), and never invest while sitting on unsold output (light-glut gate).
Industry stages are single-worker with `batches_per_worker = 1` (capacity ≈
steady demand) at $6.00 wages; chain start prices ($7.50 / $15.00 / $22.00)
sit at the audited steady-state books so no stage starts underwater.

**Consequences.** The full economy — both chains — runs healthy through
year one (16 employed, industry solvent, tools trading). The chain's
long-run persistence is a known limitation: multi-month wheat troughs still
pause tool demand below the three shops' cash runway, and once their
owners' savings are spent nothing restarts a dead business until Phase 2
(entry) / Phase 3 (credit bridges illiquidity) / Phase 4 (demand
stabilizers). Flagged in PROGRESS.md rather than papered over with
scripted revival.

## 014 — Demand-side stabilizers: comfort consumption and idle-capacity pricing

**Context.** Phase 1's larger town exposed two absorbing failure modes that
Phase 0's stabilizers (#011) do not cover. First, a hoarding leak: with
consumption fixed at one meal a day, every agent running a wage surplus
saves forever, circulation drains into idle balances, and aggregate demand
decays until no wage bill is payable — total collapse with money conserved
($16,200 stranded with households). Second, a monopoly ratchet:
produce-to-target contracts supply in lockstep with shrinking demand, so a
single-seller stage (the mill) never gluts; its stockout-raises compound
one-way, food reached ~5× wages, and the town starved while the mill
profited.

**Decision.** Two mechanics. (1) *Comfort consumption*: an agent holding
cash ≥ $400.00 (`COMFORT_CASH_FLOOR`, above starting cash so worldgen
causes no day-one shock) buys and eats a second daily meal, never into
hunger. Hoards now recycle into demand — the pool businesses sell into is
elastic in wealth. (2) *Idle-capacity pricing*: on review, a business with
no scarcity signal, a non-loss window, and expected sales below half its
bare-handed capacity (tool bonus excluded — else upgrades read as idle
capacity) cuts price 2% to chase volume. Loss-making businesses idle
capacity rather than price below cost.

**Consequences.** Seed 42's ten-year run now holds a stable, humane food
economy: both farms, the mill and the bakery staffed throughout, food back
near start price, hunger confined to the structurally unemployed. These are
behavioral rules, not subsidies — a profitable monopolist still charges
what the market bears; it just also competes with its own idle capacity.

## 015 — Food spoilage: proportional per-holder decay, perishable buffers, and the glut-boundary bug

**Context.** Phase 1's inventory work requires food spoilage. Cohort (FIFO
age) tracking would blow up the scalar inventory model, complicate market
execution, and bloat hashes for little Phase 1 value. And spoilage
interacts hard with two existing rules: the produce-to-target buffer and
the glut price signal.

**Decision.** Per-good daily decay in basis points (`Good::spoilage_bp` —
food 400, everything else durable), applied per holder at the end of the
consumption phase, rounding toward zero: **the sub-unit remainder stays
fresh**, so pantry-scale stocks never rot. Burns go through the goods
ledger (conservation targets stay exact) and are journaled per good in
metrics. Producers of perishables target `PERISHABLE_TARGET_DAYS (2) + 1`
days of stock instead of the durable 4 + 1.

Two calibration findings are locked in with this ADR, both discovered by
time-series analysis (`sim-cli metrics`, added for this purpose):

1. *The glut-boundary bug*: the durable production buffer (5 days) equalled
   the light-glut threshold (> 5 days), so every fully-buffered producer
   flirted with weekly 2% cuts and — once spoilage tightened demand flows —
   deflated wheat to the floor over years. `GLUT_LIGHT_DAYS` is now 6: the
   signal sits strictly above the normal buffer.
2. *The larder rule*: a 1-day perishable buffer is thinner than the supply
   chain's own oscillation (the mill's 3-day input-batch buying), producing
   a recurring town-wide empty-shelf day; when two outage beats landed
   close together the mill — the thinnest-margin stage — bled out and the
   chain unzipped. The perishable buffer must cover the upstream
   oscillation period: 2 + 1 days, whose ~2-3 units/day of rot is a minor
   cost at bakery margins.

**Consequences.** Year one is fully healthy and year four shows both farms
alive with real larders everywhere; the ten-year equilibrium is the
familiar harsh one (one farm dead, the survivor pricing as a monopolist,
structural hunger) — the Phase 2 entry/mobility work is what upgrades that
ending. Spoilage adds a steady replacement-demand flow (rot must be
rebaked) that slightly deepens the wheat/flour markets. Save blobs from
before this ADR are incompatible (`MetricsDay` gained fields; metrics now
carry per-business daily series for analysis and the future business
inspector).

## 016 — Business books: lifetime cash-basis accounting with a reconciliation invariant

**Context.** Phase 1 requires business accounting (P&L, balance sheet,
cash flow), and Phase 3's bank will score credit from it — so the numbers
must live in hashed state, not the journal (DECISIONS #003 discipline).
Deriving statements from the transaction ring fails (it is capped at 10k
entries), and an unverified side-tally would silently drift from reality.

**Decision.** Each business carries `Books`: starting cash plus cumulative
revenue, input costs, tool costs, wages, dividends, owner investment, net
monetary policy, and spoiled units (physical write-down, outside the cash
identity). Flows are categorized exactly at their existing ledger sites; a
new `business_books` invariant requires `cash == books.expected_cash()`
for every business on every sweep, so any cash flow that bypasses
categorization halts the simulation with that account's transaction
context. Tools are split from inputs by good (no Phase 1 recipe consumes
tools; revisit if one ever does). Statements are derived views: the
snapshot carries the books plus a balance sheet valuing inventory at last
market prices (fallback: own posted price for the sold good) — valuation
is presentation, never accounting state. Books influence no decision in
Phase 1.

**Consequences.** Verified zero behavioral impact: the seed-42 year-one
trajectory is identical to the cent with and without books. The CLI
summary and the businesses table now surface lifetime operating profit and
total assets — which immediately quantified the calibration picture
(year 1: bakery ≈ $17.7k lifetime profit, mill ≈ $7.4k, farms ≈ $5.4k,
the industry chain within ±$60 of break-even). Save blobs from before
this ADR are incompatible; hashes shift (`Business` grew).

## 017 — Construction chain: homes as one-shot durable assets; the boom is the design

**Context.** Phase 1's third chain (lumber camp → wood · brickworks →
bricks → construction company → buildings, per BRIEF). Buildings need a
buyer and a purpose. Business-side buildings (warehouses, factories) have
no mechanical effect until expansion strategy (Phase 2) and inventory has
no capacity model; households are the honest Phase 1 customer. The demand
arithmetic is unforgiving: a ~29-person town buys at most a couple dozen
homes ever, so no calibration makes a three-business chain permanently
self-sustaining on housing alone.

**Decision.** `Good::Home` is a durable asset (9 goods total): built from
6 wood + 6 bricks, sold at a posted price like any good, bought **once**
per household when cash crosses `HOME_CASH_FLOOR ($600.00)`, paying at
most half its cash (`HOME_BUDGET_SHARE_BP`). Ownership is a flag on the
agent; owned homes stay in the goods-conservation totals. Lumber camp and
brickworks are tool users (BRIEF: tools boost construction), widening the
industry chain's demand. The construction sector runs at $6.00 wages;
population rises to 29 (three new owners, the three unemployed staff the
chain). Homes are excluded from the price chart (an asset that trades
monthly is not a line); they appear in the markets table.

**Consequences.** The chain is a deliberate **boom industry**: year one
sees a real housing boom (~8 homes; the construction company is briefly
the town's most profitable per-worker business), recycling the largest
household hoards back into the wage cycle — after comfort meals, the
second demand-side stabilizer. When everyone above the floor owns a home,
the sector idles and sheds workers; without business exit/entry
(Phase 2), rehiring for later demand works but the long idle is absorbing
for the material producers. Same flagged-limitation family as the
industry chain. The year-ten food core is unharmed — best equilibrium
observed (both farms staffed, food below start price).

## 018 — Worldgen scales businesses with population

**Context.** Phase 1's acceptance requires a 100-agent, 20-business world;
worldgen previously built a fixed business set regardless of population,
so large runs were mass-unemployment proxies rather than economies.

**Decision.** Each business template carries `per_population`: one
instance per that many agents (ceiling division, minimum one), expanded
in template order with sequential ids, per-instance names from a pool
(` No.k` suffix when a pool wraps), and one owner-agent per instance.
Population clamps to the fixed point covering all owners plus one worker.
Divisors — farms 15, mills 35, bakeries 30, each specialist shop 100 —
are calibrated so the audited 29-person default reproduces **exactly**
(2 farms, 1 mill, 1 bakery, 6 shops) and 100 agents yield exactly 20
businesses (7 farms, 3 mills, 4 bakeries, 6 shops) with a wage bill
($358/day) comfortably inside the base spending pool ($540/day).

**Consequences.** The Phase 1 acceptance test
(`hundred_agent_twenty_business_year_is_green`) runs in the regular suite
with every-tick invariant sweeps. At 100 agents the first year shows real
multi-firm competition (a bakery fails while three thrive; the lone tool
factory prices as a monopolist). The 1,000-agent perf world now builds
~190 businesses and still runs ten sim years in 0.82 s release.

## 019 — Decision engine v1: utility-scored price review, traits in narrow threshold bands

**Context.** Phase 2 requires a deterministic utility-based decision engine
with stored, explainable records (AGENT_DESIGN.md). Rather than build it in
the abstract, it takes over one real decision first — the weekly price
review — so the engine wraps verified behavior before new actions arrive.

**Decision.** Agents carry nine personality traits (integer 0–100, rolled
from a dedicated per-agent `"traits"` substream in fixed field order).
`decision::score_price_action` scores {Raise, CutHeavy, CutLight, Hold} —
floats, the one sanctioned float zone; ties break by enum order, which
encodes the old cascade's priority. Neutral traits (50) reproduce the
Phase 1 rule family exactly. Traits act two ways: multiplicative weights
settle conflicts between competing signals, and **narrow threshold bands**
make personality matter on ordinary days — greed slides the stockout-raise
threshold ±0.4 days, aggression slides the idle-capacity cut threshold
between 42% and 58% utilization. Every review journals a `DecisionRecord`
(inputs, all scores, choice) with an `explanation()` renderer for the
inspector; records are outputs — saved, never hashed, never read back.
Twin-run determinism now asserts decision-sequence equality.

**Consequences.** Seeds are economically distinct for the first time (RNG
previously only named people): three seeds soak to year ten with the food
core staffed and food prices ranging $4.56–$6.92. Band width is a hard
lesson recorded here: at ±20 points of utilization, timid owners cut at
healthy mill utilization and two of three towns deflated to collapse —
traits must decide the ambiguous calls, never the clear ones. Wage and
dividend reviews stay rule-based, moving onto the engine in later
increments alongside new actions (job switching, entry/exit).

## 020 — Labor mobility: switch premiums and duration-decaying reservation wages

**Context.** Phase 0/1 workers took the first job in id order and never
left voluntarily — no pressure ever reached employers through the labor
market. AGENT_DESIGN.md requires seek/quit as engine actions.

**Decision.** Weekly per-agent job reviews (same 7-day stagger as business
reviews), in agent id order, executed immediately so later reviewers see
updated rosters. *Employed*: switch to the best open vacancy (highest
wage, tie → lower id, same marginal cash gate hiring uses) whose wage
clears a loyalty-widened premium of 10–20% over the current wage.
*Unemployed*: hold out above a reservation wage of 0.5–1.5× the going
food price by ambition, **decaying linearly to zero over a
patience-scaled horizon (30–90 days of unemployment)** — pride is a
wasting asset, so holdouts can never permanently block restaffing —
and collapsing to zero at once under desperation (hunger, or savings
under a month of food). Matching honors reservations; declined-offer
holdouts and switches journal `DecisionRecord`s (`JobReview` detail);
switches emit `JobSwitched` events.

**Consequences.** Wage differentials finally move people: underpaying
businesses lose staff to rivals and feel vacancy pressure. The flip side
is honest and kept: businesses cutting wages through a trough now lose
workers mid-crisis and can die faster (seed 42's Riverside Farm dies
~year 4 where the captive-labor world kept it alive; seeds 7 and 123 hold
their year-ten cores). Labor flight from failing firms is real economics;
the missing counterweight is business entry/exit — the next increment —
which turns dead firms from absorbing states into vacancies for the next
founder.

## 021 — Entry/exit v1: takeover-revival of moribund businesses, demand-gated

**Context.** Dead businesses were absorbing states — the root of every
long-run degeneracy (industry-chain death, construction's post-boom idle,
farm monopolies, and mobility's labor-flight deaths). AGENT_DESIGN's
action space includes start/close/acquire.

**Decision.** Exit is dormancy, not deletion: firms persist as buyable
assets. Weekly, on the agent stagger, a wealthy non-owner with
entrepreneurial appetite (ambition + risk tolerance > 120 — personality
picks WHO founds; wealth rations) may buy the highest-asset-value
**moribund** business — no staff, and even the sitting owner's savings
cannot fund one hire (the owner has daily first refusal via the injection
rule). Price is asset value (cash + inventory at market), paid owner to
owner through the ledger (`TxKind::BusinessSale`, `BusinessSold` event,
`Takeover` decision record); business books are untouched because the
firm's own cash never moves. The buyer leaves any wage job, needs price +
two hires of restart capital + a personal reserve, and the ordinary
injection/hiring machinery restarts the firm the same tick. The seller
becomes a job seeker — social circulation both ways.

Two gate lessons are locked in. *No gate*: buyers serially acquired firms
whose markets had no demand (dead tool factories), quitting productive
jobs to burn savings on zombies — whole towns collapsed. *Too strict*
(standing demand > standing offers): a dead firm's own leftover stock
masks the coming shortage and blocks the exact revival that restores
competition. The shipped gate is standing demand > 0 for the firm's good.

**Consequences.** The best small-town matrix observed: all three seeds
hold 13-employed year-ten cores — including seed 42, whose
mobility-induced farm death is cured by revival restoring duopoly
competition. The 100-town's decade-scale regression initially blamed on
takeover churn turned out to be the price deadlock diagnosed and fixed in
#022 (ownership telemetry showed only ~20 sales a decade, concentrated on
firms that kept dying for a different reason).

## 022 — The zero-revenue price deadlock and its breaker

**Context.** Ownership telemetry (BizDay owner/wage columns in the metrics
CSV) revealed the 100-town's real decade-scale disease: the year-one
demand surge spikes food to ~4× wages, pricing out most households — and
then prices FREEZE. Every corrective is structurally silent at zero sales:
stockout raises need sales, glut cuts need stock, and the idle-capacity
cut is profit-gated (#014's "cut from strength only") while a zero-revenue
window is never profitable. A mill held flour at $42.45 for six simulated
years while the whole town starved; takeovers merely recycled corpses
through the frozen market.

**Decision.** A price-deadlock breaker in the engine's heavy-cut score: a
**run of `DRY_WINDOWS_BREAKER (3)` consecutive zero-revenue review
windows** while holding stock or staffing capacity forces a heavy cut
regardless of profitability — a price earning exactly nothing cannot lose
revenue by falling, so the profit gate does not apply. `Business` tracks
`dry_windows` (hashed state), extended at each review with zero window
revenue and reset by any sale.

The run length is load-bearing and was learned the hard way: firing on a
single quiet week turned normal duopoly alternation (one batch-buyer
served by whichever farm is cheapest) into leapfrogging price wars that
razed every town in the matrix. One dry week is noise; three is a dead
price.

**Consequences.** Best full matrix to date. Seed 42 reaches its healthiest
ending yet (13 employed, 13 hungry, food $3.93); seeds 7/123 hold their
cores; the 100-town un-freezes — employment ~13–15 through the decade
(from 0–8) with food repricing from $23 toward $9.73 as the market grinds
back toward affordability. The 100-town stays harsh (≈94 hungry): ~80%
structural unemployment without welfare is Phase 4's problem by design.

## 023 — Agent memory v1: bounded grievance store, decisive only where it should be

**Context.** Phase 2 requires per-agent memory (AGENT_DESIGN §Memory) —
and PROGRESS's standing rule: memory must be load-bearing from day one,
not decorative.

**Decision.** `memory.rs`: a bounded store (12 entries) on each agent, in
hashed state because decisions read it. Memories form **at event sites**
(never by reading the journal back): being unpaid (importance 90) and
being fired (importance 70). Repetition reinforces — confidence restored
to 1000 milli, importance +10 capped at 100 — never duplicates. Phase 10
of the tick order activates: every memory loses 2 milli-confidence per
day (full → forgotten in 500 days) and forgotten memories drop; when the
store is full the weakest (importance × confidence, ties → oldest) is
evicted. Deterministic inaccuracy exists only as this explicit confidence
decay. The v1 consumer: **an active grievance (strength ≥ 20) makes a
non-desperate agent refuse to work for that business** — in matching and
in switch targeting — until the memory fades or desperation (hunger,
savings under a month of food) overrides pride. The spec's
emotional/trust/financial-impact fields and tags arrive with
relationships, their first consumer; adding them now would be dead
weight.

**Consequences.** The full soak matrix is unchanged to the cent —
grievances form and fade without flipping outcomes in healthy runs (at
100-scale chronic desperation overrides pride; in small towns stiffing
employers usually die before rehiring), while the end-to-end test proves
the decisive case: a payroll-failing bakery, solvent again, stays shunned
by its ex-staff until one goes broke and another forgets. This is
reputation's precursor: personal, private, and earned.

## 024 — Relationships v1: seven private dimensions, bond-adjusted retention

**Context.** Phase 2 requires private dyadic relationships (AGENT_DESIGN:
trust, affection, fear, respect, resentment, dependence, commercial
reliability), separate from public reputation. The PROGRESS suggestion to
route them through Memory impact fields was superseded: updating relations
directly at interaction sites is simpler and leaves Memory unchanged.

**Decision.** `relationships.rs`: sparse per-agent maps (cap 16, hashed
state; strangers implicitly neutral at 50 on every dimension; the
most-neutral relation is evicted when full). All seven dimensions have
live drivers at existing interaction sites: daily wage paid (reliability
+1), unpaid walkout (trust −30, resentment +30, fear +5, reliability
−40), hire (trust +5, dependence +20), fired (trust −10, resentment +20,
fear +15, dependence −30), weekly tenure (affection +1, dependence +1),
wage raise/cut (respect +2 / resentment +5), takeover deal (respect +10,
trust +5 both ways), leaving a job (dependence −30). Phase 10 drifts
every dimension one step toward neutral on the agent's weekly stagger day
(acquaintance fades in about a year) and drops fully-neutral relations.
The v1 consumer: the job-switch premium becomes
`loyalty premium + bond_premium_bp` (trust + affection + dependence −
resentment, ×3 bp, clamped ±500, floor 200) — attachment binds,
resentment repels, and neutral relations reproduce prior behavior
exactly.

**Consequences.** Retention is now earned: long-tenured, well-treated
workers need visibly better offers to leave (test: identical wages and
loyalty, only the private bond differs — the stranger takes the raise,
the bonded worker stays), and wage-cutting employers accumulate
resentment that cheapens poaching them. Seeds 7/123 and the 100-town hold
their envelopes; knife-edge seed 42 lands on a harsher branch this run
(10 employed, food ~$21.67 at year ten — same alive-and-trading family it
has oscillated within across every behavioral increment; per-seed ending
selection is explicitly not a tuning target).

## 025 — Reputation as propagated belief; `probe_reputation` guards the channel

**Context.** Phase 2 requires public reputation spread through observation
and rumor, distinct from private relations, with `probe_reputation` as the
acceptance probe (TEST_PLAN: probes assert a propagation channel exists,
never a scripted outcome).

**Decision.** Reputation is not a global score: each agent holds bounded
BELIEFS about others (`reputation.rs`, cap 16, hashed state, strangers
neutral, most-neutral eviction, weekly drift toward neutral). Dimensions
with live drivers ship — **reliable** (payroll observed +1/day, missed
payroll −25), **generous** (wage raise +5 / cut −5), **ruthless** (fired
+20); honest/competent/wealthy/dangerous/influential/corrupt arrive with
their drivers (contracts Phase 3, politics Phase 4). Propagation: on each
agent's weekly stagger day they LISTEN to roster colleagues plus their two
id-neighbors (the workplace and the neighborhood — the neighborhood venue
exists because victims stranded jobless or on solo rosters otherwise have
no audience), moving a quarter of the gap toward each speaker per subject.
**Neutrality is silence**: speakers only voice beliefs of intensity ≥ 8,
so ignorant consensus cannot erase firsthand knowledge — only competing
news can. Consumer: a non-desperate job seeker refuses owners believed
unreliable (< 26) — grievance generalized socially; desperation overrides.

**Consequences.** `probe_reputation` (pinned seed 42) passes by latching
the trajectory: the machinery produces a payroll failure, victims hold
firsthand beliefs, and a non-witness's belief moves below neutral through
gossip — while opinions still legitimately fade and compete afterward.
The pinned run's emergent biography is worth recording: the bakery's
disgraced owner lost the firm in a takeover, was later observed dutifully
paying wages at a business he briefly revived, and ended the run as a
mill worker with a mixed public record. The soak matrix is unchanged —
reputation bites exactly where it should (after public failures) and
nowhere else.

## 026 — Contract kernel v1: requirements-form supply contracts, and the five collapses that shaped them

**Context.** Phase 3 opens with the contract kernel: a `Contract` entity
in hashed state, ONE type end to end (the supply contract), deterministic
settlement in the reserved tick phase 6, breach with ledger penalties,
and a `contract_reconciliation` invariant. The naive design — fixed
weekly quantity at a fixed price for a quarter — passed every unit test
and killed every town it touched. Each collapse was diagnosed from
`sim-cli metrics` CSV time series against a pre-contract baseline
worktree, and each fix is a recorded mechanic, not a tuning nudge.

**The form.** A supply contract locks a unit PRICE (the seller's posted
price at signing minus a 5% commitment discount, gated by the buyer's
input reservation cap so a contract can never lock in a price the spot
market would refuse) and a DAILY CEILING; each day the buyer takes its
current input need up to the ceiling (requirements form). Deliveries
settle daily in phase 6: goods seller→buyer as a zero-sum trade, cash
buyer→seller through the ledger (`TxKind::ContractDelivery`); books
categorize at the site. A zero-need day settles trivially. A failed take
is a miss: the failing side (seller first when both fail) pays a
cash-capped 25% penalty; three consecutive misses terminate as Breached.
84 scheduled days ≈ a quarter; expiry is v1's renegotiation. Committed
ceilings are withheld from the seller's market offers, added to its
production target, and netted out of its glut and tool-gate signals
(committed stock is sold stock in waiting); buyers protect today's take
in their market budget. Formation is a weekly utility-engine decision on
the buyer's review stagger (Sign vs StaySpot; greed weighs the discount,
low risk tolerance buys supply security, gamblers hold out until cover
thins), take-it-or-leave-it at the cheapest capable seller — the
offer/counteroffer log is the next increment. Sellers auto-accept up to
an 80% capacity share (floored at one unit so single-worker stages can
contract at all); buyers refuse sellers they publicly believe unreliable
(the #025 floor). An underwater buyer — locked price past the
reservation cap beyond an honesty-widened tolerance (0–10%) — walks
away, paying the exit penalty (`Terminated`, journaled as a
`ContractExit` decision). Contract performance drives relationships
(commercial reliability both ways) and reputation (misses and walk-aways
seed "unreliable" beliefs — the BRIEF's contract-performance channel).

**The five collapses.**
1. *Weekly lumps* starved the hand-to-mouth chain between due dates (the
   seed-6 famine): sellers withheld a week's committed stock — including
   from the waiting buyer itself. → Daily cadence.
2. *Committed-seller stockout ratchet*: counting residual unmet spot
   demand as scarcity gave contracted sellers a stockout day every day —
   a one-way price ratchet no glut could correct (seed-7 food
   inflation). → Stockout marks require zero TOTAL stock, as before
   contracts.
3. *Fixed-quantity anchor*: with sales exactly equal to contracted
   inflow, every stage's EMA-derived orders equaled the contract
   forever — a stable under-production fixed point (farms selling 4/day
   beside 16 free units, towns starving next to stock). → Requirements
   form, plus the demand-pull channel: recent stockout days add
   one-for-one to planned production and input orders, so shortages
   propagate upstream as quantities, not only prices.
4. *Wage ratchet on the dead* (latent pre-contract bug the deeper
   contract crises exposed): a zero-activity business has window profit
   exactly 0, passed the old `>= 0` raise gate, and bid +5% weekly to
   the $10,000 ceiling — pricing both rehiring and takeover revival out
   of existence for the whole town. → Raises need strictly positive
   profit, and an offer the till can't fund walks down 3% weekly.
5. *Staffed-zombie deadlock*: a firm hovering just above the hire floor
   paid wages forever with zero input budget; its rich owner never
   triggered the hire-gap injection, and its silent input demand kept
   every upstream supplier failing the revival gate. → The "invest"
   action's second slice: owners inject working capital when a staffed
   recipe business can't fund a day of inputs; and the takeover demand
   gate counts contract-committed flow as live demand.

**Scope choice.** Even with all five fixes, food-chain (wheat/flour)
contracts collapsed every 10-year pop-29 soak: households are
price-takers for survival food, so no reservation cap disciplines the
chain downstream and every distortion lands in its razor-thin cash
margins. Industry and construction contracts, by contrast, left towns at
least as strong as before. v1 therefore scopes contracts to durable
industrial inputs (`contracts::contractable`); food-chain contracts
return when the bank can float working capital. Recorded, not silent.
Final matrix: seeds 42/7/123/6 × 3650 all land at 13 employed with
hunger 12–21 — on par with the strongest pre-contract matrix (#022's 13
employed) across MORE seeds, and the pop-100 decade ends at 13 employed
with less hunger than before (83 vs ≈94).

**Consequences.** Saves break (SimState, Books, TxKind, Event gained
fields/variants; pre-1.0 policy, no released saves). All hashes shift
(calibration + state shape). Terminal contracts stay in state as the
contract view's archive (tiny; revisit retention if soaks grow it). Flat
`Contract` fields factor into a terms enum when the second type lands.
Deferred to later increments: offer/counteroffer negotiation logging,
seller-side strategic breach, multi-sourcing one good across sellers,
renegotiation before expiry, and the contract view UI.

---

## 027 — Bank v1: distress credit, default→foreclosure, and the rate lever

**Context.** Phase 3's remaining acceptance criteria: the
default→foreclosure flow test and `probe_rate_shock`. The BRIEF's banking
spec (deposits, loans, interest, collateral, credit assessment, defaults,
foreclosures, liquidity, solvency) is bigger than one increment; this
slice ships the credit kernel end to end and records the rest.

**Decision.** One bank, capitalized at worldgen ($70/resident, minted as
part of the initial money supply) with its own ledger account
(`AccountId::Bank`, appended after Business/Agent so buyer ordering is
untouched), its own lifetime books, and a `debt_reconciliation` invariant
(bank cash == books; per-loan balance/counter/state identities; loan-book
sums == bank aggregates) every sweep.

*Loans.* Working-capital term loans to businesses: 84 days, straight-line
principal, annual rate in bp fixed at issue (360-day year), interest
accrued daily in integer MILLI-cents on the declining balance (sub-cent
remainders carry in an accumulator and never become money until paid
through the ledger; a repaid loan's dangling sub-cent dies with it).
Service collects daily in tick phase 7, interest before principal, full
payment or a counted miss — `DEFAULT_AFTER_MISSES (3)` consecutive misses
default the loan. Foreclosure: seize cash up to the claim, then inventory
goods at last market prices (whole units — lumpy, the ≤1-unit overshoot
is the borrower's loss; unpriced goods cannot settle debt), write off the
rest against bank equity. Seized goods sit in bank inventory (goods
conservation counts it) and fire-sell daily to the market's own
deterministic buyer queue at last prices (off-market: `last_prices`
unmoved). The stripped business survives as an ordinary moribund firm —
takeover-revival's problem, not the bank's.

*Demand side.* Borrowing is the distress ladder's third rung: own till,
then owner injection, then — if the business still cannot fund a hire or
a day of inputs — the owner scores **Borrow vs Struggle** through the
utility engine (`BorrowReview`): payroll runway sets the urgency, the
rate is the price, debt aversion (low risk tolerance) weighs it. This
price sensitivity IS the transmission channel: `probe_rate_shock` (pinned
seed 42, `SetBankRate` to 150% at tick 100) shows organic lending in the
control run and contraction in the shocked run. Businesses with no
payroll clock (no workers) get no urgency — the bank does not do venture
lending for restarts; injection and takeover cover those.

*Supply side.* Deterministic assessment, no RNG: refuse a second
concurrent loan, refuse any borrower with a defaulted loan on the book
(v1 credit memory — permanent; soften when credit history matters more),
refuse below the bank's liquidity floor (25% of starting capital — as
defaults eat equity the lendable pool shrinks: the credit-contraction
capability the BRIEF asks for), and require the income test (day-one
service ≤ 50% of expected daily revenue) OR the coverage test (principal
≤ 70% of assets). Rate = the posted base rate at issue; the player's
`SetBankRate` command (clamped 0..=50,000 bp) reprices future loans only.

*Priority.* Loan service is junior to wages, market reserves and contract
takes (phase order 4→5→6→7); a borrower's due service is protected in its
market budget like a contract payment, so a miss means real insolvency,
not sloppy sequencing.

**Verification.** Natural credit lives: seeds 42/7 organically issue
loans from ~day 112 (up to 2–3 concurrent) and produce 6 natural
defaults each with conservation green. The staged flow test
(`foreclosure.rs`) walks borrow → service → 3 misses → default →
seizure → fire sale → writeoff through full ticks. The pop-29 soak
matrix holds at 13 employed on all four seeds. The pop-100 decade
drifted between dystopia variants (13 → 7 employed at year 10; its
formal acceptance — the 1-year scale.rs run — stays green); that horizon
belongs to Phase 4's `soak_10y` + welfare levers and is recorded, not
chased here.

**Deferred, recorded.** Deposits and bank runs (liquidity crises),
household credit and mortgages, takeover/expansion lending, bank
ownership and dividends, credit-history decay, multi-bank competition.
Saves break again (SimState/Books/TxKind/Event/PlayerCommand grew);
schema_version stays 1 pre-1.0.
