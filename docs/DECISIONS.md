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

---

## 028 — Negotiation v1: a logged three-round haggle, and the contract view

**Context.** The BRIEF demands real negotiation ("deterministic offers
and counteroffers, considering market price, bargaining power, …,
personality") with COMPLETE logging, surfaced as a history table in the
contract view. v1 formation was take-it-or-leave-it at a flat 5%
discount.

**Decision.** Supply-contract formation now runs a bounded, integer,
three-round haggle (`negotiation.rs`), anchored entirely in observable
state — no RNG. The buyer opens 6%–12% under the seller's posted price
(greed stretches the anchor); the seller holds a reserve floor 2%–8%
under posted (greed narrows the concession); convergence follows explicit
rules — accept an opening at/above the floor, counter partway from
posted, split the difference capped by the buyer's input reservation
ceiling, then the seller's bottom line; a floor above what the input can
earn back is an impasse and the buyer walks. Every move is journaled
with its mover, price and reason in a `NegotiationRecord` ring
(`Journal.negotiations`, cap 2,000 — outputs, never hashed, never read
back). The buyer sits at ONE table a week (the cheapest capable,
non-distrusted seller); an impasse retries next review at whatever
prices then hold.

The **achieved discount** — not a constant — feeds the existing
Sign/StaySpot review, so outcomes now compose three ways: impasse at the
table, agreement the buyer's review then declines (`BuyerDeclined` — a
stingy seller can win the haggle and lose the deal), or a signature
(`Signed { contract }` — the pointer the contract view uses to find the
table talk). The flat `CONTRACT_DISCOUNT_BP` is gone.

**The contract view** (the Phase 3 UI deliverable): the snapshot carries
the newest 50 contracts (parties by name, good, daily ceiling, price,
delivered/missed tallies, state chip); clicking a row opens the contract
inspector over the on-demand detail protocol (`get_contract_detail` →
`ContractDetail`): terms, tallies, penalties, the negotiation log move
by move, and the contract's event history (delivery/miss/breach/
termination — bounded by the events ring, so ancient contracts honestly
say the record scrolled away).

**Verification.** Launch-verified at three zoom levels: day 26 (live
delivery events "…delivered 1 iron ore … under C3"), year 88 (the table
showing breached/completed states), and a breached contract's inspector
(signed Y1·D282, 15/84 delivered, missed 3, penalties $5.64, the full
haggle from "buyer opened below spot — $7.17" to "seller gave the bottom
line — $7.53 / buyer accepted — $7.53"). Incidental soak evidence: the
debug shell ran ~260,000 consecutive ticks with every-tick invariant
sweeps at max speed without a halt — the pop-29 world is stable at 13/19
employed through simulated centuries. The full matrix is unchanged;
`probe_rate_shock` and both flow tests held without recalibration.

**Deferred.** Wage negotiation through the same protocol (the labor
market's reservation-wage machinery is negotiation-shaped already);
seller-side personality beyond greed (desperation should widen the
floor); multi-seller shopping; renegotiation mid-term. The negotiation
inspector POLISH (dedicated screen) stays v1.1 per the BRIEF.

---

## 029 — Government kernel v1: a cascading sales tax, a welfare floor, and the 300 bp lesson

**Context.** Phase 4 opens with the fiscal kernel: a government account,
ONE tax end to end, a budget that spends on something real, and the
command plumbing (PROGRESS session 6's exact next task). The BRIEF binds
two constraints: the government "cannot spend unlimited money", and tax
collections must reconcile. The phase order is pinned in CLAUDE.md, so a
new fiscal phase is itself a recorded decision.

**Decision — the account.** `SimState.government` with
`AccountId::Government` appended after `Bank` (neither places market
orders; buyer ordering is untouched). Unlike the bank it is **born
broke**: the treasury holds only what taxation collected — no worldgen
minting, so the budget constraint is structural, not policed. Its
lifetime `GovBooks` (tax_collected, welfare_paid, policy_net) must imply
the treasury exactly.

**Decision — the tax.** A seller-side sales tax in integer basis points,
collected at BOTH revenue sites — market execution and contract
settlement (contracting must never be a tax dodge) — at the same ledger
statement that books the seller's revenue, so the seller always holds at
least the tax it owes. `tax = value × rate / 10000` rounded toward zero;
the sub-cent remainder explicitly stays with the seller. Exempt by
design: liquidation fire-sales (distress recovery, not commerce),
contract penalties (damages), wages, dividends, business sales
(income/capital taxation are separate future levers). `Books.taxes_paid`
joins the business cash identity and `lifetime_profit`;
`PlayerCommand::SetSalesTax` clamps to 0..=10,000 bp and reprices from
the tick it applies.

**Decision — the spending.** A daily welfare floor in a new tick phase 8
(banking → **government** → consumption; CLAUDE.md's pinned order and
ECONOMIC_RULES both updated): every agent below `WELFARE_FLOOR` ($12.00
≈ two days of food) is topped up to it, most destitute first (cash,
then id), until the treasury runs dry — the dole covers eating, nothing
else, and the marginal recipient gets whatever is left.

**Decision — the invariant.** `tax_reconciliation`: treasury == books;
non-negative fiscal totals; rate inside the clamp; and Σ business
`taxes_paid` == government `tax_collected` — every collected cent has a
payer who booked it. That last sum makes `taxes_paid` globally
load-bearing: scenario surgery that resyncs a business's books mid-run
must carry it forward (probe_reputation's staging now does, re-basing
starting cash — the invariant caught it on first contact).

**The 300 bp lesson (options weighed by experiment, not argument).** The
first calibration (300 bp, floor always-on) collapsed two of the four
standing seeds to dead towns by year 4 — while the same build at rate 0
reproduced the baseline exactly on all four. Bisecting with
welfare disabled isolated two distinct failure modes:

1. *The wedge*: a seller-side turnover tax **cascades** down a chain —
   wheat, flour and food each pay, so 300 bp ≈ a 9% wedge on food's
   final value, more than the calibrated margins bear. Seed 6 died of
   the tax alone.
2. *The recycling distortion*: the always-on floor recycles the entire
   take into FINAL food demand, propping consumer prices while the
   intermediate stage's margin collapses — on seed 123 the mill bled
   out beside a farm holding 22 unsold wheat, then the staffed bakery
   starved of flour and the town died. Without welfare the deeper
   deflation re-cleared the chain and 123 survived.

At **100 bp** all four seeds hold the baseline's 13 employed through the
decade under every dole variant tried. A pantry-targeted dole (pay only
the actually-starving) was tried and REJECTED: measured over the last
500 ticks it was identical to the plain cash floor on three seeds and
six agents worse on seed 42 — targeting bought complexity, not relief.
Final: rate 100 bp, plain means test. Steady state: the treasury pins at
zero (everything collected pays out daily), ~0.9 recipients/day, ~$7,000
redistributed per decade, and seed 42's mean hunger drops from ~20 to
~14. The rate lever stays fully in the player's hands — pushing it back
to 300 reproduces the collapse, which is exactly the "policies have
costs" emergence Phase 4's probes want.

**Consequences.** Money conservation and metrics totals include the
treasury; the metrics/CSV grow govt_cash, tax_collected, welfare_paid,
welfare_recipients. Saves break (SimState/Books/TxKind/Event/
PlayerCommand grew); schema_version stays 1 pre-1.0. The pop-100 decade
issue stays open (13/100 employed at year 10 — top of the recorded
band; a 1% dole cannot fix a structural scaling problem). Deferred to
later Phase 4 increments: government debt/deficits, the remaining
levers (income/business tax, minimum wage, subsidies, spending
programs), welfare as a settable lever, and the UI surface (Phase 5 owns
screens; the shell still has no command channel).

---

## 030 — Scenario shocks: conditions, not outcomes — and where a drought must bind

**Context.** Phase 4's deterministic event system. The BRIEF's contract
is strict: "events modify underlying conditions, never prescribe
outcomes — a drought reduces agricultural output; the resulting food
shortage, inflation and business failures must emerge from normal
systems." Tick phase 2 has been a reserved slot since Phase 0.

**Decision — the lifecycle.** A shock is a `TriggerShock { kind, days }`
player command (so scenarios ride the existing command log: queued,
tick-boundary applied, replayable, deterministic — no new scheduling
machinery and no RNG). It lives in hashed state (`SimState.shocks`) for
a fixed span; phase 2 retires it the day it expires, giving exactly
`days` modified production days. One shock per kind at a time —
re-triggering is a `CommandRejected`, because silent stacking is how a
scenario stops meaning what it says. `ShockBegan`/`ShockEnded` journal
both ends.

**Decision — one mechanical hook.** Shocks touch the economy through a
single function: `capacity_bp(state, business_kind)` — a production-
capacity multiplier. Drought: farms × 50% (`DROUGHT_CAPACITY_CUT_BP =
5_000`). Two sites apply it, deliberately BOTH: the production batch cap
(the condition itself) and the price review's utilization base — a
farm producing at half capacity is not idle, and without the second
site the anti-monopolist idle-capacity cut would fire price CUTS into
the scarcity the drought just created.

**Decision — where the probe binds.** The first calibration run
injected the drought at tick 200 and the economy shrugged: output fell
17%, no price moved — because tick 200 is the post-boom glut, where
farms run far under capacity and buffers are fat. That is the system
WORKING (a drought during a glut should be absorbed), but a probe
wants its channel demonstrable, so `probe_drought` injects at tick 600
— the mature steady state, production sized to demand — where the cut
binds hard: output 56% of control, wheat peak +69% over its pre-drought
mean (control flat), food +28%, 34 food-chain price raises. Thresholds
frozen with regression margins (≤75%, ≥130%, ≥115%, ≥5). The tick-200
lesson is recorded in the probe's own comments.

**Verification.** All 19 suites green; the four-seed decade soak
reproduces the government-kernel endpoints EXACTLY (an untriggered
shock system is behavior-neutral — phase 2 is a no-op on an empty
list). Saves break again (SimState grew `shocks`); schema_version
stays 1 pre-1.0.

**Deferred.** The BRIEF's remaining shock kinds land opportunistically
with the mechanics they touch (mine collapse → capacity_bp on mines;
epidemic → labor; export boom → demand side, which needs a second hook
kind); shock magnitude as a command parameter (constants keep scenarios
comparable); shocks in the snapshot/overview UI (Phase 5); the contract
capacity-share check reads nominal capacity — revisit when a
contractable kind can be shocked.

---

## 031 — soak_10y and the delayed effect this economy actually has

**Context.** Phase 4's last two test deliverables: the decade soak with
non-degeneracy bands, and the BRIEF's delayed-policy-effect test
("policies have costs, tradeoffs and delayed effects").

**Decision — what "business exit and entry" means here.** The BRIEF's
soak asks for "at least one business exit and one entry", but v1 never
founds or deletes businesses (worldgen creates them; the takeover
machinery recycles them — pinned since Phase 1). `soak_10y` therefore
asserts the economy's ACTUAL churn channels: an exit is a staffed
roster emptying (death), an entry is a dead roster staffing back up
(injection/takeover/rehiring revival). Calibrated on seed 42's decade:
12 deaths, 6 revivals — both channels demonstrably alive. The other
bands, calibrated to the real steady state and frozen: food produced
and trading at the end; at least one staple repricing within the last
500 ticks (wheat takes 3 distinct prices while food rests at its
equilibrium — a fully frozen tape is the degenerate state guarded
against); ≥8 employed / ≤14 unemployed at the end (actual 13/6).

**Decision — which delayed effect to pin.** The obvious scenario (hike
the sales tax, watch the contraction arrive late) turned out to be
WRONG about this economy, and the finding is worth the record: in the
mature steady state both seed 42 and the marginal seed 123 absorb even
a 9% sales tax indefinitely — employment pinned at 13, hunger flat,
business cash HIGHER under the hike — because the welfare floor
recycles the entire take straight back into final demand. Taxation
plus a full-recycling dole redistributes; it does not contract. (ADR
#029's collapses happened at 300 bp from tick 0 — the wedge is lethal
only in the fragile consolidation era, before the steady state forms.
WHEN a policy lands matters as much as its size.)

The delayed effect this economy genuinely exhibits runs the other
direction: **abolish the welfare state and nothing happens — for a
season.** `policy_lag` pins that shape on seed 42: `SetSalesTax { 0 }`
at tick 600 stops the dole within days (cause immediate: 100 payments
per 100 days → 0), yet the first fortnight and the first quarter read
within +0.15 and +0.10 hungry of control — floats, pantries and
standing prices carry the poor for a while. The cost arrives on a
~500-tick fuse: by [1100, 1500) the abolished run averages +5.7 hungry
(20.1 vs 14.3) — the no-welfare equilibrium of the E0 calibration
reappearing on schedule. Frozen guards: ≤1.0 divergence through the
first quarter, ≥3.0 by year three.

**Consequences.** Phase 4's test skeleton is complete: all four
emergence probes (`probe_reputation`, `probe_rate_shock`,
`probe_drought`, `soak_10y`) plus `tax_reconciliation` and the
delayed-policy test are green. Remaining phase scope is now purely
mechanics: the "all policy levers" set (which needs a v1 scoping
decision — several BRIEF levers ride systems that do not exist) and
government budget/debt.

---

## 032 — The v1 lever set, sovereign debt, and the poverty-debt trap

**Context.** Phase 4's close-out: the BRIEF asks for "all policy
levers" and a government that "cannot spend unlimited money without
explicitly creating debt or money". Several BRIEF levers name systems
v1 does not have — "all" needs an honest scoping decision, recorded
here rather than silently shrunk.

**Decision — the v1 lever set.** Shipping: `SetSalesTax` (#029),
`SetBankRate` (#027), `AdjustMoneySupply` (Phase 0 — and, targeted at
an account, it IS the BRIEF's "emergency relief"), `TriggerShock`
(#030), and three new levers: **`SetWelfareFloor`** (0..=$100; the dole
becomes policy — $0 legally abolishes it), **`SetMinimumWage`**
($3..=$100; the statute is the wage review's floor, and a
non-compliant posted wage is forced up on its next review — whether
the till can afford compliance is the business's problem, which is the
policy's emergent cost; the statutory minimum can never go below the
$3.00 mechanical floor it replaces), and **`SetDeficitLimit`**
(0..=$100,000; see below). Recorded as out of v1 scope, tied to the
mechanics they need: income/business taxes (additional collection
sites; the tax ARCHITECTURE is proven end to end), subsidies (a
spending program beyond welfare), antitrust (no merger mechanics),
contract-enforcement and bankruptcy-rule variation (the penalty and
foreclosure parameters exist but as constants — parameterizing them is
mechanical when a driver appears), import/export (a closed economy by
design until an external-trade system exists).

**Decision — sovereign debt.** The deficit lever: with
`debt_limit > 0`, a treasury that cannot cover the day's welfare bill
draws the shortfall from the BANK — capped by the limit's headroom and
by the bank's own liquidity floor (a drained bank rations the state
like any borrower; no money is created). The debt floats at the bank's
CURRENT base rate (`SetBankRate` prices the deficit too), accruing in
integer milli-cents like business loans. The fiscal day's fixed order:
interest first (whatever the treasury cannot pay CAPITALIZES into the
principal — the state does not default, its debt compounds), then
borrowing, then the dole, then any surplus retires principal (an
indebted treasury never hoards). Default `debt_limit` is ZERO — a
balanced budget, bit-identical to pre-lever behavior. New machinery:
`SovereignDraw`/`SovereignService` transactions, `GovBorrowed`/
`GovDebtCleared` events, `govt_debt` metrics column, and
`tax_reconciliation` extended with the debt identity (outstanding ==
drawn − repaid + capitalized) plus a cent-for-cent cross-check of the
bank's sovereign books against the treasury's.

**The poverty-debt trap (found by the cycle test, kept by design).**
The fiscal-cycle test killed the intake with the lever open (the dole
ran on credit, interest compounded — all as designed), then restored
an 8% intake expecting repayment. It never came: the credit era left a
backlog of destitution whose daily bill consumed every cent of revenue
before the principal, forever — the dole-first priority turns heavy
debt plus mass poverty into a self-sustaining trap. This is real
sovereign-finance behavior emerging from three simple rules, and it
STAYS: the test's repayment leg now runs an austerity program
(`SetWelfareFloor{0}` alongside the restored intake) and retires the
debt in a month, which is itself the tradeoff the BRIEF wants players
to face.

**Verification.** 140 sim-core unit tests + 22 suites green;
`fiscal_cycle` covers the full borrow→compound→repay arc, town-wide
minimum-wage compliance within one review cycle, and every lever's
clamp through the command channel. The five-run soak matrix is
UNCHANGED to the agent (13 employed everywhere; hungry 14/15/23/20;
pop-100 decade at 13) — all three levers default to bit-neutral
values. Saves break again (Government/BankBooks/TxKind/Event/
PlayerCommand grew); schema_version stays 1 pre-1.0.

**Deferred.** Government bonds held by households (the bank is the
sole sovereign creditor in v1); a debt-ceiling invariant (deliberately
absent — capitalization may push debt past a lowered limit, which only
gates NEW draws); welfare eligibility rules beyond the cash means
test; the levers' UI surface (Phase 5).

---

## 033 — `sim-cli serve`: the websocket transport, sync and thread-per-client

**Context.** Phase 5 opens with the reserved second transport: the
Playwright E2E suite drives the React app in a real browser, so the
browser needs a real backend. ARCHITECTURE.md reserved a websocket
implementation of the shell's protocol since Phase 0.

**Decision — sync, no async runtime.** `tungstenite` (blocking) over
std threads, mirroring the desktop shell's shape exactly: one sim
thread owns the `World` (sim-core stays single-threaded), one accept
thread, one thread per client. The per-client thread avoids locked or
split sockets entirely: it pumps its outbound mpsc queue and polls the
socket under a 50 ms read timeout in one loop. A tokio stack for a
local dev/E2E tool would be three dependencies of ceremony for zero
benefit; if serve ever needs hundreds of clients it earns a runtime
then.

**Decision — the wire shape.** JSON text frames; client messages carry
an optional `req` id and get a correlated `{kind:"reply", req, ok,
data|error}`; snapshots push as `{kind:"snapshot", data}` — on
connect, at the 10 Hz throttle while running, and to every client
after each handled message so a driver sees its effect immediately. A
malformed message answers with `ok:false` echoing whatever `req` the
envelope carried (a driver's request fails; it never hangs).

**Decision — the command channel ships here first.** `queue_command`
accepts any `PlayerCommand` via serde's external tagging and queues it
for the next tick boundary — the transport-level superset the Phase 5
policy screen and E2E "apply a policy" step need. The desktop shell
gains the same channel when that screen lands; until then serve is
deliberately ahead of it (recorded, not drifting).

**The client half.** `app/src/ipc.ts` is now genuinely
transport-agnostic: Tauri via dynamic imports, or the serve websocket
in any plain browser (default `ws://127.0.0.1:17771`, `?ws=`
override), with request/reply correlation and reject-on-disconnect —
vitest-covered against a scripted fake socket (5 tests). The app's
no-backend screen now says how to start serve.

**Verification.** A Rust integration test drives the full protocol
with a real websocket client (ephemeral port): snapshot-on-connect,
speed change, tick advance, `SetSalesTax` queued at the next boundary,
a malformed command refused without hanging, agent detail, save to a
temp dir, and a second client's immediate snapshot. Launch-verified in
a real Edge browser against `serve` + vite: the world ran to Y2·D231
live over the socket — stats, chart, tables, and the Phase 4 welfare
events streaming in the log.

**Deferred.** Loading a save into serve (`--load`, with the save-slot
increment); TLS/remote binding (localhost-only by design);
backpressure beyond the OS socket buffer (snapshots are ~tens of KB at
pop 29 — revisit with the 1000-agent perf pass).

---

## 034 — The macro stats and the policy panel (what "GDP" and "inflation" mean here)

**Context.** The BRIEF's world overview names macro indicators the
snapshot never carried — GDP, inflation, wealth inequality, interest
rate, government budget — and the Phase 4 levers existed only as
CLI/test commands. Phase 5's first screen work: make the macro picture
visible and the levers pullable.

**Decision — honest definitions over textbook ones.** Each stat is
defined by what this economy actually measures, documented on the
field: **GDP (7d)** = spot-market trade value summed over the last
seven metric days (reconstructed as Σ volume × daily VWAP; contract
deliveries settle off-market and are excluded — a derived view, never
read by simulation logic). **Food inflation (90d)** = mean traded food
price over the last 14 days vs the 14-day window ending 90 days ago,
in basis points; `None` until both windows have trades, so the UI
shows an honest em-dash for the first 97 days. **Wealth inequality** =
Gini of household cash in basis points (sorted-rank formula, exact
zero on perfect equality; business equity stays on the business
table). Interest rate, treasury, sovereign debt and every lever value
are direct state reads — the panel's readbacks come from the same
snapshot the world pushes, so an enacted lever visibly changes its own
row.

**Decision — the policy panel.** One "Government" panel: budget line
(treasury + sovereign debt) and five lever rows (sales tax, bank rate,
welfare floor, minimum wage, deficit limit), each showing the current
value, a unit-labelled input (dollars/percent at the surface, integer
cents/bp on the wire), and an Enact button that calls `queueCommand` —
the reply's tick renders as "enacted — takes effect day N", the same
next-tick-boundary contract every mutation obeys. The desktop shell
gained the matching `queue_command` Tauri command (`ShellMsg::
QueueCommand`, the sim loop's `&World` closure widened to `&mut`), so
both transports now carry the full protocol.

**Verification.** Launch-verified in the browser against serve, end to
end: the sales-tax lever enacted 1% → 5% from the panel, the readback
updated on the next snapshot, the note reported "takes effect day
123", and the treasury's growth visibly steepened — with the welfare
floor pinning two destitute agents at exactly $12.00 on the agent
table, the first time the dole is visible in the UI. All suites green
(140 unit / 16 vitest; the store fixture grew the ten new stat
fields). The desktop shell's channel is compile-verified with the
identical handler pattern as its four proven commands; its interactive
check was deliberately skipped because the machine was in active use
during verification (fighting the user's foreground window is worse
than deferring) — the packaged-app smoke test (Phase 6) exercises it.

**Deferred.** A GDP that counts contract-settled flow; a basket price
index (food is the survival good and the honest v1 signal); wealth
Gini including business equity attributed to owners; per-lever
confirmation dialogs (commands are reversible by counter-command).

---

## 035 — The city view is presentation, not state — and the timeline gets filters

**Context.** Two BRIEF v1.0 screens: the stylised 2D city map and
event-timeline filters. The simulation has NO spatial model — nothing
in `sim-core` knows where anything is, and inventing coordinates in
hashed state for a picture would be scope masquerading as data.

**Decision — the map is a pure derived view.** `CityView` lays the
snapshot out deterministically by kind and id: farmland, town,
industry and works zones of business tiles (name, kind, staffing;
dead businesses get a red dashed border), a civic column (the bank
with its rate, the government with its treasury), and a residential
strip with one house glyph per resident — filled when they own their
home, hollow when renting, flushed red while hungry. Clicking a house
opens the agent inspector. No distances, no routes: the BRIEF's
"transport routes" ride a real spatial model if one ever exists
(recorded, not faked). Layout wraps by row counts, so pop-100 worlds
render without code changes.

**Decision — timeline filters are kind groups plus text.** The event
log gains chips (People / Business / Contracts / Finance /
Government) mapping the ~30 event kinds into player-meaningful
groups, plus a free-text filter over the rendered event text (names,
goods, anything). A kind missing from every group still shows under
"All" — new event kinds degrade gracefully instead of vanishing.
Filtering is client-side over the snapshot's event tail; deeper
history rides the SQLite archive when the replay/archive UI lands.

**Verification.** Launch-verified in the browser against serve at two
world states: at day 64 the map showed a fully staffed town with the
hungry cluster matching the stats chip; by day 180 it had LIVE
emergence on screen — the lumber camp and brickworks dead with red
dashed borders, the hungry house count grown, the treasury at $0. The
Government filter chip activated with an honest "nothing matches this
filter" empty state, and clicking a red filled house opened the
inspector on exactly a hungry homeowner (Lars Kroll, pantry 0,
hungry 1d). Both gates green; no sim-core changes at all in this
increment.

**Deferred.** Clicking a business tile (needs the business inspector
screen — the next Phase 5 batch alongside historical charts);
warehouse/route glyphs (no spatial model); map zoom/pan (a static
overview earns its keep first).

---

## 036 — The business inspector: the books, made a screen

**Context.** The last missing v1.0 inspector. The BRIEF's business
model names P&L, balance sheet, cash flow, valuation — and every one
of those already EXISTS in state as the lifetime cash-basis `Books`
(reconciled every sweep by `business_books`), the market-valued
inventory helper (the same number takeovers pay), and the bank's loan
book. The screen's job is presentation, not new accounting.

**Decision.** `BusinessDetail::capture` on the established on-demand
protocol (its third client): identity and staffing; pricing and
expectations; the FULL lifetime books as signed categorized flows
(inflows positive, outflows negative — dividends, tax remitted,
interest, seizures all visible); a balance sheet at market valuation
(cash + inventory = assets, loan outstanding = liabilities, equity
the difference — asserted equal in the unit test); credit standing
(active loan terms and misses, or prior defaults — "the bank
remembers"); contracts on both sides with roles; and the newest 40
events touching the business (`event_touches_business` resolves
contract events through their parties; JobSwitched matches either
side). Cash flow gets no separate statement: cash-basis books ARE the
cash flow (recorded, not missing). Reachable three ways: business
table rows, city map tiles, and (later) anywhere else a business is
named. A `.expanded` panel class gives an open inspector the stack's
slack — the four-panel column had squeezed every body to zero height
once the city panel took the top of the viewport.

**Verification.** Unit test (seed 42, day 200): staffed, revenue on
the books, the balance sheet balances, history capped and present,
missing id → None. Launch-verified in the browser via a city-tile
click: Stonebridge Mill's inspector rendered live — "3/3 staff at
$4.61/day · week $22.59 · lifetime $4,399.94", balance sheet "cash
$595.39 · inventory $297.84 · assets $893.23 = equity", "on hand: 15
wheat · 21 flour". Final capture taken by `PrintWindow` BACKGROUND
capture: the machine was in active use again mid-verification, and
one automation click landed in the user's editor (caret move only) —
foreground automation stops the moment the user is active, full
stop; background window capture is the polite tool and it worked.
Both gates green (141 unit tests).

**Deferred.** Historical per-business charts (BizDay series exist in
metrics — next batch); linking inspector contract rows to the
contract inspector across panels; wage/price history sparklines.

---

## 037 — Historical charts and named save slots (the world learns to rewind)

**Context.** Phase 5's last two feature screens: historical charts and
save-slot management with an autosave cadence. The metrics journal
already held every series; sim-persist already knew how to save, load
and read a save's meta. The work was surface and protocol, not new
state.

**Decision — the macro chart rides the snapshot.** `MacroHistory`
(employment, hunger, treasury, sovereign debt) over the same bounded
window as `price_history`, rendered by `MacroChart` on a second tab of
the chart panel — counts on the left axis, money on the right, same
dataviz idiom as the price chart. No on-demand query needed at this
window size; per-business historical charts stay deferred.

**Decision — slots are files, the protocol stays symmetric.** A slot
name is a sanitized filename (alphanumeric/dash/underscore, ≤32; a
hostile "../evil" is refused with an error). Both transports carry
`save {slot}`, `load {slot}` and `list_saves` (slot + saved tick via
`read_meta`); loading swaps the sim thread's world in place and the
next push shows every client the rewound state. `sim-cli serve
--load <path>` starts from a save. Autosave is WALL-CLOCK, 60 s, from
the shell/serve loops (never inside sim-core, never per tick —
CLAUDE.md), skipped while paused via a tick-unchanged check, into the
"autosave" slot. The UI's Save button became a Saves menu: three
player slots (save/load, saved date shown) plus load-only rows for
autosave/quicksave/anything else on disk. Loading rewinds; determinism
means replaying forward from the same save reproduces the same run —
the existing save/resume equality tests already guard exactly that.

**Verification.** The serve integration test now runs the full arc
over a real websocket: save to "alpha" → run forward at max speed →
load "alpha" → the snapshot REWINDS to the saved tick; the listing
carries both slots; the hostile slot name is refused. All suites green
(141 unit, 16 vitest, tsc). In-browser: both new surfaces mount and
render (background `PrintWindow` capture — the machine was in active
use, both foreground attempts were correctly aborted, and Edge
exposes no child HWND for background clicks). The Saves panel's
click-through and the Society tab's canvas are exactly two of the
steps the Playwright E2E suite (next increment) automates — recorded
as deferred to it, not skipped silently.

**Deferred.** Save-file deletion/renaming from the UI; autosave
rotation (one slot suffices at these file sizes); cloud/scenario
export; per-business chart series.
