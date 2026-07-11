# Plan — Phases & Acceptance Criteria

The authoritative scope is `BRIEF.md`; this file tracks the build order and
what "done" means per phase. A phase is complete only when its acceptance
tests pass, `npm run check:full` is green, and nothing in it is a placeholder.

## Phase 0 — Foundations and vertical slice  **[in progress]**

Workspace per CLAUDE.md; `npm run check` / `check:full`; the docs set;
simulation kernel (clock, ChaCha substreams, command queue + log, canonical
BLAKE3 hashing, invariant framework); minimal food chain (2 farms → mill →
bakery → household consumption) with 20 agents and 4 businesses; employment
and daily wages; posted-price markets for wheat/flour/food; cash ledgers with
money-conservation invariant; save/load via sim-persist; determinism tests
(twin-run hash equality; save/resume equality; replay equality); working
Tauri UI: overview stats, agent table, business table, price chart, event log.

*Done when:* all slice tests pass, the app launches and shows a living
economy, and PROGRESS.md reflects it.

## Phase 1 — Full economy

All three chains (food, industry, construction); 8–12 goods; generalized
production; inventory with food spoilage; construction producing buildings;
households and needs; business accounting (P&L, balance sheet, cash flow);
goods-conservation invariants; market view v1.

*Done when:* ore→steel→tools→farm-productivity integration test passes;
inventory/goods reconciliation tests pass; a 100-agent, 20-business world
runs one sim year headless with all invariants green.

## Phase 2 — Agent society

Full utility-based decision engine (complete action set, stored decision
records with explanations); memory (decay, reinforcement, deterministic
inaccuracy); relationships (seven private dimensions); reputation (public
dimensions + propagation); complete agent inspector.

*Done when:* utility-scoring unit tests pass; `probe_reputation` passes; a
real decision's explanation is visible in the inspector; determinism suite
still green.

## Phase 3 — Contracts and finance

Full contract lifecycle for all seven types; deterministic negotiation with
complete offer/counteroffer logging; breach and penalties; the bank
(deposits, credit assessment, loans, collateral, defaults, foreclosure,
liquidity, solvency); debt and contract reconciliation invariants; contract
view.

*Done when:* supply contract negotiated and fulfilled end-to-end in an
integration test; default→foreclosure flow test passes; `probe_rate_shock`
passes.

## Phase 4 — Government, events and emergence

All policy levers; government budget and debt; deterministic event system
(shocks modify conditions only); tax reconciliation; delayed-policy-effect
test; `probe_drought`; `soak_10y`.

*Done when:* all four emergence probes and the soak test pass alongside the
full invariant suite.

## Phase 5 — UI completion and analytics

All v1.0 screens polished (world overview, city view, agent inspector,
business inspector, market view, contract view, event timeline); speed
controls; save-slot management + autosave cadence; historical charts;
timeline filters; `sim-cli serve` (websocket protocol); Playwright E2E suite.

*Done when:* E2E green and a written per-screen manual checklist verified in
the running app.

## Phase 6 — Hardening and release (v1.0)

Property-based tests (proptest); the full failure-test list; performance pass
against every target in PERFORMANCE_PLAN.md (profile first); packaged Windows
build via the Tauri bundler; packaged-app smoke test; final docs; tagged
v1.0.

## v1.1 backlog (strictly after the v1.0 tag)

Scenario branching + comparison dashboard · replay inspector UI ·
relationship network graph · negotiation inspector polish · optional
feature-flagged LLM narrative layer.
