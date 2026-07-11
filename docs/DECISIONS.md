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
