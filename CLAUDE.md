# Marketborn — Project Constitution

Deterministic economic-agent strategy simulation. Desktop app: Tauri 2 + React + TypeScript UI, Rust simulation core, SQLite persistence. This file is **binding for every session**. The full product brief lives in `docs/BRIEF.md`; the current project state lives in `docs/PROGRESS.md`.

## Session protocol

Every session, in this order:

**Start**
1. Read `docs/PROGRESS.md` (current state + exact next task), the current phase in `docs/PLAN.md`, and skim `docs/DECISIONS.md`.
2. Run `npm run check` from the repo root. If it fails, fixing it is the first task of the session. Never build on red.

**Work**
3. Do one bounded increment toward the current phase's acceptance criteria. Finishing one thing beats starting three.
4. Any non-trivial decision → record it in `docs/DECISIONS.md` (context, options, choice, consequences) before moving on.

**End — mandatory, reserve ~10% of the session for this**
5. Run `npm run check`. A session may not end red on main. If work is genuinely unfinishable, revert to the last green state or park it on a `wip/` branch and say so in PROGRESS.md.
6. Update `docs/PROGRESS.md`: what changed, actual test results, benchmark numbers if touched, unresolved issues, and the exact next task — written for a reader with zero memory of this session.
7. Commit with a conventional-commit message (`feat:`, `fix:`, `test:`, `docs:`, `perf:`, `refactor:`).

If context is running low mid-task: stop at a coherent point and do the end ritual early. A clean handoff beats a bigger diff.

## Hard rules

- No placeholders, mocks, stubs, or fake data in any feature marked complete.
- Never claim tests pass without running them. Never claim the app works without launching it (or exercising the E2E path for UI work).
- Do not silently reduce scope. If a tradeoff is needed, make it, record it in PROGRESS.md, and flag it.
- Fix root causes, not symptoms. Every significant bug gets a regression test.
- No `any`, no `@ts-ignore`, no `unwrap()`/`expect()` in non-test sim code, no `#[allow(...)]` without an inline justification (and a DECISIONS.md entry if systemic).
- Simulation logic never lives in React components. The UI never mutates simulation state — every world-changing action goes through the command queue.
- Determinism is never traded for convenience.
- Warnings are errors: `clippy -D warnings`, `tsc` strict mode.
- Do not start v1.1 or stretch features before the v1.0 tag exists.
- Keep files maintainable; split modules before they exceed ~500 lines.

## Pinned technical decisions

Change any of these only with a DECISIONS.md entry explaining why.

- **Workspace layout**: `crates/sim-core` (pure simulation logic — no I/O, no wall clock, no threads), `crates/sim-persist` (SQLite save/load/replay/event archive), `crates/sim-cli` (headless runner, replay, hash diff, benchmarks, `serve` mode), `src-tauri/` (thin shell hosting the sim thread), `app/` (React + Vite + TS), `docs/`.
- **Time**: 1 tick = 1 simulated day. Every tick executes a fixed, documented phase order: apply queued commands → scheduled events → production → labor market → goods markets → contract settlement → banking → consumption → agent decisions → memory/relationship updates → metrics, invariant checks, hashing. The phase order is part of the determinism contract and lives in `docs/ECONOMIC_RULES.md`.
- **Money**: `i64` minor units (cents) everywhere. Rates as integer basis points. Division rounds toward zero; remainders are explicitly assigned to a party (never dropped) so conservation invariants hold. No floating point in ledgers, prices, balances, or any accounting path.
- **RNG**: `rand_chacha::ChaCha8Rng`, version-pinned in Cargo.toml. A master seed derives named substreams (hash of seed + stream name + entity id) per system and per agent, so adding a feature never reshuffles unrelated randomness. No RNG use outside `sim-core`.
- **Determinism scope**: same build on the same platform (Windows is primary). Same seed + same initial config + same command log ⇒ identical state hashes and identical event sequence. Floats are permitted in decision utility scoring only — never in accounting. Cross-platform bit-exactness is explicitly not a v1 goal.
- **Concurrency**: `sim-core` is single-threaded. Parallelism, if ever, is a post-1.0 project with its own determinism design.
- **State vs persistence**: authoritative world state lives in memory. SQLite is the save/replay/event-archive format only — no live reads or writes inside the tick loop. Autosave on a cadence, never per tick.
- **UI boundary**: a transport-agnostic protocol. Inbound: `PlayerCommand`s, queued, applied only at tick boundaries, appended to the command log. Outbound: throttled `WorldSnapshot` summaries (≤10 Hz) plus on-demand detail queries (inspectors fetch by entity id). Never serialize the full world at frame rate. Tauri IPC and `sim-cli serve` (websocket) both implement this same protocol.
- **Testing transports**: Rust unit/integration/property/determinism tests in the workspace; Vitest for UI logic; Playwright E2E drives the React app in a real browser against `sim-cli serve`; a small smoke test covers the packaged Tauri app.
- **IDs and iteration**: stable integer entity IDs. Any iteration that affects simulation outcomes uses ordered structures (`Vec`, `BTreeMap`) or an explicit sort. `HashMap` iteration order must never touch simulation results.
- **State hashing**: canonical serialization of world state → BLAKE3. Hash every N ticks (default 50) and at every save. Run manifests record (tick, hash) pairs; `sim-cli diff` compares two runs and reports the first divergent tick with commands/events context.

## Quality gates

`npm run check` = `cargo fmt --check` + `cargo clippy -D warnings` + `cargo test --workspace` + `tsc --noEmit` + `vitest run`. Keep it under ~5 minutes. Slow suites (soak, property-based, benchmarks) live behind `npm run check:full`, which must be run and green before any phase is declared complete.

Economic invariants (money, goods, debt, ownership, tax, contract reconciliation) run every tick in debug builds and on the hash cadence in release builds. An invariant failure pauses the simulation and emits a diagnostic report: tick, failing invariant, expected vs actual, the deltas, and the last 50 transactions touching the affected accounts.

## Docs map

- `docs/BRIEF.md` — full product brief (source of truth for scope)
- `docs/PLAN.md` — phases and acceptance criteria · `docs/PROGRESS.md` — living state, updated every session
- `docs/DECISIONS.md` · `docs/ARCHITECTURE.md` · `docs/DATA_MODEL.md` · `docs/ECONOMIC_RULES.md` · `docs/AGENT_DESIGN.md` · `docs/TEST_PLAN.md` · `docs/PERFORMANCE_PLAN.md` · `docs/PERF_RESULTS.md`
