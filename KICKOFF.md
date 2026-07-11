# Marketborn — Product Brief & Session 1 Kickoff

You are the principal game architect, simulation engineer, AI systems designer, UI designer, QA lead and technical writer for this project. Your task is to design and build **Marketborn**: a production-quality, local, offline desktop strategy simulation about autonomous economic agents — part economic strategy game, part living city simulation, part financial analytics dashboard.

`CLAUDE.md` at the repo root is **binding for every session** — read it before anything else. It pins the session protocol, hard rules, workspace layout, tick semantics, money representation, RNG, IPC design, and the determinism contract. This brief defines *what* to build; CLAUDE.md defines *how you work*. During setup, copy this file to `docs/BRIEF.md` so future sessions can reference it.

Do not produce a prototype that only looks convincing. Build a functioning simulation with coherent economic rules, deterministic outcomes, persistent saves, real agent decision-making, automated tests and a polished interface.

# Product vision

The player oversees a fictional city-state populated by autonomous agents. The player does not directly control characters — they observe, influence through policy, and experiment with a society of agents who own businesses, negotiate contracts, form alliances, compete for resources, remember interactions, develop reputations and react to changing conditions.

Agents can: earn, spend, save, invest and borrow money; own homes, businesses and productive assets; work for companies; hire and fire; buy and sell goods; produce through supply chains; negotiate contracts; form partnerships and alliances; compete with rivals; remember significant interactions; hold private opinions about other agents; carry public reputations; react to prices, shortages, unemployment and shocks; expand businesses; acquire competitors; default on debts; go bankrupt; accumulate political influence; and change strategy as circumstances evolve.

The world must be capable of producing emergent outcomes such as: monopolies, recessions, inflation, resource shortages, labour disputes, wealth concentration, cooperative business networks, political factions, price wars, debt crises, business collapses, trade booms, corruption, social mobility, and long-running personal rivalries.

**These outcomes must emerge from the simulation's normal systems. Never script them, and never hardcode outcomes that should emerge.**

# Release staging

**v1.0 (this project's Definition of Done):** the complete simulation — all three production chains, full agent society (decisions, memory, relationships, reputation), contracts and negotiation, banking and debt, government and policy, deterministic save/load/replay with CLI diff tooling, the full v1.0 UI screen set, the complete test suite, recorded benchmarks, and a packaged Windows build.

**v1.1 (do not start before the v1.0 tag):** scenario branching with a comparison dashboard, the replay inspector UI (visual two-run diff), the relationship network graph, negotiation inspector polish, and the optional language-model narrative layer.

The replay, hashing and diff *machinery* is v1.0 — the determinism tests depend on it. Only the rich UIs on top of it are deferred.

# Architecture

Stack and workspace layout are pinned in CLAUDE.md (Tauri 2, React, TypeScript, Rust, SQLite; `sim-core` / `sim-persist` / `sim-cli` / `src-tauri` / `app`). Supporting tools: Zustand for UI state, TanStack Query for inspector detail queries if useful, ECharts for charts, SVG or Canvas for the city view (adopt PixiJS only if profiling proves it necessary — record in DECISIONS.md), Vitest, Playwright, Rust test framework, `proptest` for property-based testing, `tracing` for logging.

The Rust simulation engine is fully independent of the interface. The same core powers three modes:

1. Desktop graphical mode (Tauri)
2. Headless CLI mode (`sim-cli run`)
3. Automated test mode

Modules, at minimum: world state · simulation clock · deterministic RNG · agent model · business model · employment system · resource system · production system · inventory system · market system · contract system · banking and debt · memory system · relationship system · reputation system · agent decision engine · event system · government and policy · save/load · replay · analytics and metrics · UI layer.

Core economic logic never lives in React components. The UI never mutates simulation state. All world-changing actions pass through explicit `PlayerCommand`s or simulation systems.

# Time and market mechanics (pinned)

**Tick semantics.** 1 tick = 1 simulated day. Each tick runs a fixed phase order (see CLAUDE.md); document the final order in `ECONOMIC_RULES.md` — it is part of the determinism contract. Recurring schedules (wages, interest accrual, price reviews) run on fixed tick cadences defined in ECONOMIC_RULES.md.

**Market mechanism: posted prices.** Each good has a registry of standing sell offers (seller, unit price, quantity) posted by businesses and agents. During the market phase, buyers compute their demand (household needs plus production inputs), then purchase in deterministic order — urgency tier first, then entity id — each taking the cheapest available offer that fits (ties broken by lower seller id), subject to available funds or explicitly created credit. Sellers review prices on a cadence (e.g., every 7 ticks): stockouts and high sell-through push prices up, accumulating inventory pushes them down, with bounded step sizes modulated by personality (greed, patience). Market metrics derive from executed trades and standing offers.

**Labor market.** Vacancies and job seekers match in the labor phase in deterministic order; reservation wages meet offered wages, with negotiation running through the contract system.

# Determinism contract

Determinism is non-negotiable, scoped as pinned in CLAUDE.md: **same build, same platform**. A simulation started from the same seed, initial world configuration, command log, and simulation version must produce the same world state and event sequence.

Requirements:

- Seeded `ChaCha8Rng` with named substreams per system and per agent (see CLAUDE.md) — never global or ad-hoc RNG.
- Ordered iteration everywhere outcomes are affected; stable integer entity IDs.
- No wall-clock time inside simulation logic. Ticks are explicit.
- Every player command and external input is recorded in the command log with its tick. The command log is part of the save.
- Deterministic replay: initial state + command log reproduces the run exactly.
- Deterministic continuation: save at tick T, load, run to tick U ⇒ identical hash to an uninterrupted run to tick U.
- State hashing: canonical serialization → BLAKE3, every N ticks (default 50) and at every save. Run manifests store (tick, hash) pairs.
- Automated determinism tests (see Testing).

**CLI tooling (v1.0):** `sim-cli run` (headless), `sim-cli replay <save>`, `sim-cli hash <save> --at <tick>`, `sim-cli diff <runA> <runB>` — the diff reports the first divergent tick with the commands processed, events emitted and random draws around it. The graphical replay inspector built on this data is v1.1.

# Economic integrity

Money, goods and debt are accounted for consistently. Money never appears or disappears without an explicit source or sink. All amounts are `i64` minor units; rates are integer basis points; rounding remainders are explicitly assigned (CLAUDE.md).

Every transaction records: buyer · seller · amount · goods or service exchanged · tax · fees · debt created or repaid · inventory changes · ledger entries.

Maintain double-entry (or equivalently auditable) ledgers for agents, businesses, banks, the government, and markets.

Continuously checked invariants:

- Total money is conserved except for explicit monetary policy actions.
- Goods are conserved except through production, consumption, spoilage or destruction.
- Inventory quantities never go negative.
- Debt balances reconcile between lender and borrower.
- Contract payments reconcile. Tax collections reconcile. Bankruptcy transfers reconcile.
- Ownership percentages always sum validly.
- No agent spends unavailable funds unless debt is explicitly created.

On invariant failure: pause the simulation and emit the diagnostic report specified in CLAUDE.md (tick, invariant, expected vs actual, deltas, last 50 transactions on affected accounts).

# Initial economy

First playable scenario: a small industrial settlement with three production chains.

**Food:** Farm → Wheat → Mill → Flour → Bakery → Food → Household consumption
**Industry:** Iron mine → Iron ore → Steel mill → Steel → Tool factory → Tools → productivity improvements for farms, mines and construction
**Construction:** Lumber camp → Wood · Brickworks → Bricks → Construction company → Homes, shops, factories, warehouses

Initial world target: 100 autonomous agents · 20 businesses · 8–12 tradable resources · 5 economic sectors · 1 bank · 1 government · residential and industrial zones · at least 3 social factions · at least 5 personality archetypes. The architecture must support scaling well beyond these numbers.

# Agent model

Every agent has: unique identity, age, household, occupation, skills, cash, assets, debt, inventory, employment status, business ownership; personality traits (risk tolerance, time preference, loyalty, honesty, ambition, aggression, patience, empathy, greed); social and political influence; current and long-term goals; relationships; memories; public reputation; private beliefs; current needs; recent actions.

Traits influence decisions but never fully determine them — an ambitious-cautious agent behaves differently from an ambitious-reckless one. Agents must adapt when their plans fail.

# Agent decision engine

The core decision engine must not depend on a language model. Use a deterministic utility-based or goal-oriented action system.

Action space (minimum): seek employment · quit · negotiate salary · hire · fire · buy goods · sell goods · change prices · start business · expand business · close business · borrow · repay debt · invest · form partnership · offer contract · accept contract · reject contract · renegotiate contract · breach contract · acquire competitor · sell business · support political faction · spread information · punish rival · help ally · hoard resources · liquidate inventory.

Every considered action receives a utility score based on: expected profit, risk, liquidity, personality, current goals, relationships, reputation, market conditions, memory, opportunity cost, time horizon, legal consequences, political consequences. (Utility scores are the one place floats are allowed — see CLAUDE.md.)

The simulation must be able to explain why an agent chose an action. For each important decision, store: actions considered · utility per action · relevant memories · relevant relationships · economic assumptions · chosen action · outcome · whether the result changed future behaviour. Surface these explanations in the agent inspector.

# Memory system

Agents need bounded, meaningful memory. A memory contains: event · participants · timestamp · importance · emotional impact · trust impact · financial impact · confidence · decay rate · current relevance · tags.

Examples: a supplier delivered late; a partner defended the agent in a dispute; a borrower defaulted; a rival undercut prices; an official granted favourable treatment; a friend provided emergency funding.

Memories decay unless reinforced; repeated behaviour strengthens a belief. Agents may remember inaccurately if uncertainty is explicitly modelled — but inaccuracies must remain deterministic.

# Relationships and reputation

Private relationships and public reputation are separate systems. Private dimensions: trust, affection, fear, respect, resentment, dependence, commercial reliability. Public dimensions: reliable, honest, competent, generous, ruthless, wealthy, dangerous, influential, corrupt.

An agent may personally trust someone with a poor public reputation. Reputation spreads through direct observation, news, social networks, contract performance, court or government actions, business outcomes, and rumours.

# Contracts and negotiation

Implement a real contract system. Types: supply contracts, employment contracts, loans, partnerships, exclusive distribution, property leases, acquisition agreements.

Terms may include: parties, goods or service, quantity, price, duration, delivery schedule, payment schedule, penalties, collateral, exclusivity, renewal, termination conditions, breach consequences.

Agents negotiate via deterministic offers and counteroffers, considering: market price, bargaining power, urgency, alternatives, trust, reputation, liquidity, personality, previous interactions, expected future value.

Log every negotiation completely — each offer, counteroffer, and the reason for acceptance or rejection. In v1.0 this appears as a history table in the contract view; the dedicated negotiation inspector polish is v1.1.

# Business system

Businesses have: owners, employees, cash, debt, inventory, production capacity, buildings, equipment, suppliers, customers, contracts, wage policy, pricing policy, expansion strategy, profit & loss statement, balance sheet, cash flow statement, valuation, reputation, competitive position.

Businesses can: produce, hire, fire, invest, borrow, default, merge, acquire, split ownership, issue dividends, retain earnings, change prices, enter and exit markets, negotiate supplier agreements.

# Banking and debt

Implement deposits, loans, interest, collateral, credit assessment, defaults, foreclosures, bank liquidity and bank solvency. Lending decisions weigh: borrower income, assets, existing debt, business performance, reputation, collateral, economic conditions, and the bank's risk tolerance. The system must be capable of producing a credit contraction or banking crisis through emergent behaviour.

# Government and policy

The player influences the economy through policy, not by commanding agents. Levers: business/income/sales tax rates, interest rate policy, government spending, subsidies, welfare, minimum wage, antitrust enforcement, contract enforcement, bankruptcy rules, import/export policy, emergency relief.

Policies have costs, tradeoffs and delayed effects. The government has a budget and cannot spend unlimited money without explicitly creating debt or money.

# Events and shocks

Support deterministic scenario events: drought, mine collapse, resource discovery, population influx, epidemic, trade disruption, bank failure, technology improvement, labour strike, political scandal, fire, war in a neighbouring region, export boom.

Events modify underlying conditions, never prescribe outcomes. A drought reduces agricultural output; the resulting food shortage, inflation and business failures must emerge from normal systems.

# User interface

The interface should look like a premium strategy simulation crossed with a financial analytics product: warm charcoal backgrounds, muted blue accents, strong typography, clear hierarchy, compact but readable data, minimal decoration, smooth transitions, excellent desktop usability.

**v1.0 screens:**

- **World overview** — date/tick, simulation speed controls, population, GDP, inflation, unemployment, wealth inequality, interest rate, government budget, major shortages, major events.
- **City view** — stylised 2D map (SVG/Canvas): residential areas, farms, mines, factories, shops, bank, government buildings, warehouses, transport routes.
- **Agent inspector** — identity, wealth, income, debt, employer, businesses, traits, goals, relationships, reputation, memories, current reasoning, recent decisions with explanations, full timeline.
- **Business inspector** — ownership, employees, inventory, production, revenue, expenses, profit, debt, contracts, valuation, suppliers, customers, strategy, competitive position.
- **Market view** — supply, demand, price, volume, inventory, historical charts, largest buyers/sellers, shortages, surpluses.
- **Contract view** — terms, parties, payment and delivery history, breaches, penalties, renegotiations, trust effects, full negotiation history.
- **Event timeline** — filterable by agent, business, resource, contract, event type, severity, date range.

**v1.1 screens:** relationship network graph, scenario comparison dashboard, replay inspector.

All screens consume the snapshot/detail-query protocol pinned in CLAUDE.md. The UI stays at 60 fps while the sim runs at maximum speed on its own thread.

# Save, load and replay

Support multiple save slots, autosave on a cadence, manual save, and named checkpoints. A save contains everything needed to resume deterministically, including the command log and RNG state, plus human-readable metadata and a `schema_version`.

Versioning policy: before 1.0, breaking save changes are allowed but must be noted in PROGRESS.md. From 1.0 onward, saves within the same major version must load, via migrations where the schema changes.

Replay = initial state + command log. Branching from a checkpoint (loading a save into a new run manifest) is supported at the data level in v1.0; the comparison dashboard is v1.1.

# Performance

Targets (release builds, on the dev machine — record actual numbers in `docs/PERF_RESULTS.md` at every phase):

- 100 agents with UI open: UI at 60 fps while the sim runs at maximum speed; snapshots ≤ 10 Hz.
- 1,000 agents headless: 3,650 ticks (10 sim years) in ≤ 60 seconds.
- Save ≤ 2 s and load ≤ 2 s at 1,000 agents.
- Replay at least as fast as live simulation.
- Bounded memory: in-memory ring buffers for hot event data, archive spills to SQLite. No full-world cloning per tick. No database writes inside the tick loop.

Benchmark suite measures: tick duration (p50/p99), decisions per second, memory usage, event throughput, save time, load time, replay speed, UI render time. **Profile before optimising** — record profiling evidence in DECISIONS.md before any major optimisation.

# Testing

Testing is part of the product. All suites run under `npm run check` or `npm run check:full` (CLAUDE.md).

**Unit tests:** production, pricing, transactions, contracts, loans, interest (integer basis-point math and rounding assignment), reputation, memory decay, utility scoring, taxation, bankruptcy, save/load.

**Integration tests:** complete flows — e.g., mine produces ore → steel mill buys ore → produces steel → tool factory buys steel → farm buys tools → farm productivity measurably increases.

**Economic invariant tests:** money, inventory, debt, ownership, tax and contract reconciliation.

**Determinism tests:** identical seed + commands run twice ⇒ identical hashes at ticks {100, 1,000, 3,650} and identical event/transaction/decision order. Save at tick 500 → load → run to 1,000 ⇒ hash equals an uninterrupted run. Replay from command log ⇒ identical hashes throughout.

**Property-based tests (proptest):** generate many random valid worlds, run thousands of ticks, assert all invariants hold.

**Emergence probes** — deterministic scenario tests that assert propagation channels exist, not scripted outcomes. Calibrate thresholds once against a pinned seed, then freeze them as regression guards:

- `probe_drought`: inject a drought at a fixed tick; assert wheat output falls materially, wheat and food prices rise past the pinned thresholds within N ticks, and at least one food-chain business posts a negative margin or raises prices. All invariants stay green.
- `probe_rate_shock`: sharply raise the policy rate; assert new lending volume falls and at least one marginal borrower defaults within the horizon.
- `probe_reputation`: a scripted breach by agent A against B; assert B's private trust in A drops, A's public reliability degrades after propagation, and B's acceptance threshold for A's next offer rises.
- `soak_10y`: 3,650 ticks with no player commands; assert invariants hold at every check and the economy stays non-degenerate — food production positive at the end, price series not frozen, at least one business exit and one entry, unemployment within a sane band.

**End-to-end tests (Playwright against `sim-cli serve`, in a browser):** start a new world, change simulation speed, inspect an agent, inspect a business, apply a policy, save, load. Plus a minimal smoke test of the packaged Tauri app (launch, create world, ticks advance, save).

**Failure tests:** empty markets, no employers, mass bankruptcy, bank insolvency, resource exhaustion, extreme inflation, negative shocks, corrupted save file, old save version, 1,000-agent world.

# Optional language model layer (post-v1.1)

The game must function fully without any external AI API. A later, feature-flagged layer may generate negotiation dialogue, newspaper articles, biographies, decision summaries and narrative flavour — always derived from already-determined simulation events, never controlling economic outcomes, never inside `sim-core`, and never affecting determinism when disabled.

# Development phases

For every phase: implement → add tests → run the tests → launch the app and inspect real behaviour → fix defects → update docs → commit. A phase is complete only when its acceptance tests pass, `npm run check:full` is green, and nothing in it is a placeholder.

**Phase 0 — Foundations and vertical slice.** Workspace per CLAUDE.md; `npm run check`; the full docs set; simulation kernel (clock, ChaCha substreams, command queue + log, canonical hashing, invariant framework); minimal food chain (farm → mill → bakery → household consumption) with 20 agents and 4 businesses; basic employment and wages; posted-price markets for wheat, flour and food; cash ledgers with the money-conservation invariant; save/load via `sim-persist`; determinism tests (twin-run hash equality; save/resume equality); a working Tauri UI showing overview stats, agent table, business table, a price chart and an event log.
*Done when:* all slice tests pass, the app launches and shows a living economy, and PROGRESS.md reflects it.

**Phase 1 — Full economy.** All three chains; 8–12 goods; generalised production, inventory (with food spoilage) and construction producing buildings; households and needs; business accounting (P&L, balance sheet, cash flow); goods-conservation invariants; market view v1.
*Done when:* the ore→steel→tools→farm-productivity integration test passes; inventory/goods reconciliation tests pass; a 100-agent, 20-business world runs one sim year headless with all invariants green.

**Phase 2 — Agent society.** Full decision engine (complete action set, utility inputs, stored decision records with explanations); memory (decay, reinforcement, deterministic inaccuracy); relationships (all seven private dimensions); reputation (public dimensions plus propagation channels); complete agent inspector.
*Done when:* utility-scoring unit tests pass; `probe_reputation` passes; a real decision's explanation is visible in the inspector; the determinism suite is still green.

**Phase 3 — Contracts and finance.** Full contract lifecycle for all seven types; deterministic negotiation with complete offer/counteroffer logging; breach and penalties; the bank (deposits, credit assessment, loans, collateral, defaults, foreclosure, liquidity and solvency); debt and contract reconciliation invariants; contract view.
*Done when:* a supply contract is negotiated and fulfilled end-to-end in an integration test; the default→foreclosure flow test passes; `probe_rate_shock` passes.

**Phase 4 — Government, events and emergence.** All policy levers; government budget and debt; the deterministic event system (shocks modify conditions only); tax reconciliation; delayed-policy-effect test; `probe_drought`; `soak_10y`.
*Done when:* all four emergence probes and the soak test pass alongside the full invariant suite.

**Phase 5 — UI completion and analytics.** All v1.0 screens polished; speed controls; save-slot management; historical charts; timeline filters; `sim-cli serve`; the Playwright E2E suite.
*Done when:* E2E is green and a written manual checklist (one item per screen behaviour) is verified in the running app.

**Phase 6 — Hardening and release (v1.0).** Property-based tests; the full failure-test list; performance pass against every target (profile first); packaged Windows build via the Tauri bundler; packaged-app smoke test; final docs; tagged release.

**v1.1 backlog (after the tag):** scenario branching + comparison dashboard, replay inspector UI, relationship network graph, negotiation inspector polish, optional LLM layer.

# Definition of done — v1.0

- The packaged application installs and launches locally.
- A new seeded world with 100 autonomous agents and 20 businesses runs; businesses produce, hire, trade, borrow and fail; agents negotiate contracts, remember interactions, and their relationships and reputations demonstrably change behaviour (probe tests).
- All three supply chains function end to end; prices react to supply and demand; the government can change policy; shocks propagate through normal systems.
- Same seed + same commands ⇒ identical hashes (automated). Saves resume deterministically (automated). `sim-cli replay` reproduces prior runs (automated).
- Economic invariants are continuously checked; `npm run check` and `npm run check:full` are green.
- Benchmark results are recorded against every performance target.
- All docs are current, and the v1.0 tag exists.

# Working rules

The hard rules in CLAUDE.md apply at all times. In addition: do not ask for approval on ordinary implementation decisions — decide, record in DECISIONS.md, move on. Ask only when a decision is irreversible, expensive or security-sensitive. When blocked, investigate, document the cause, and choose the best technically sound solution. Do not continue building on a known broken foundation.

# First task — begin now

1. Initialise the repository and workspace per CLAUDE.md. Set up `npm run check` and `npm run check:full`, `rust-toolchain.toml`, pinned dependencies, and copy this brief to `docs/BRIEF.md`.
2. Write the docs set: `ARCHITECTURE.md`, `DATA_MODEL.md`, `ECONOMIC_RULES.md` (including the final tick phase order and all cadences), `AGENT_DESIGN.md`, `TEST_PLAN.md`, `PERFORMANCE_PLAN.md`, `PLAN.md` (the phases above with their acceptance criteria), `DECISIONS.md` (seeded with the pinned decisions from CLAUDE.md), and `PROGRESS.md`.
3. Build the Phase 0 vertical slice, fully functional and tested.
4. Perform the end-of-session ritual from CLAUDE.md. In subsequent sessions, continue through the phases without waiting for further instruction, always following the session protocol.
