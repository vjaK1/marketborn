# Agent Design

## Phase 0 (implemented)

Agents are deliberately minimal — the kernel proves determinism, ledgers and
markets before psychology arrives:

- **Households**: buy food to a pantry target (urgency rises when starving),
  eat daily, track hunger streaks, seek any job when unemployed (reservation
  wage 0), accept the first offer in deterministic order.
- **Owners**: hold one business, don't take wage jobs, receive dividends.
- **Business decisions** (the Phase 0 "decision engine", rule-based and
  documented in ECONOMIC_RULES.md): produce to inventory target; post prices
  and review weekly on stockout/glut signals; adjust wages on vacancy age and
  window losses; hire to a fixed target with a cash gate; fire LIFO in a cash
  crunch; pay dividends above a survival buffer.

Everything here is deterministic, id-ordered, and float-free.

## Phase 2 status

Implemented so far (session 2, DECISIONS #019–#020): the nine personality
traits (integer 0–100, per-agent `"traits"` substream, field order = roll
order); the utility engine core (`decision.rs`) with float scoring,
enum-order tie-breaks and journaled `DecisionRecord`s that render their own
explanations; the business **price review** runs through it (greed and
aggression in narrow threshold bands); **labor mobility** — weekly job
reviews with loyalty-widened switch premiums, ambition-scaled reservation
wages decaying over a patience-scaled unemployment horizon, desperation
override. Seeds produce economically distinct histories. Still to come:
remaining owner reviews on the engine, business entry/exit (next — the
counterweight to labor flight from failing firms), memory, relationships,
reputation, the agent inspector.

## Phase 2 target (per BRIEF.md — design direction)

### Identity & traits

Every agent gains: age, household, skills, personality traits (risk
tolerance, time preference, loyalty, honesty, ambition, aggression, patience,
empathy, greed — integer scales, worldgen-rolled from per-agent substreams),
social/political influence, goals (current + long-term), needs.

Traits *influence* utility weights, never fully determine choices; an
ambitious-cautious agent must diverge from an ambitious-reckless one under
identical conditions.

### Decision engine

Deterministic utility-based action selection, never an LLM:

1. Enumerate feasible actions from the brief's action space (seek/quit job,
   negotiate salary, hire/fire, buy/sell, price changes, start/expand/close
   business, borrow/repay, invest, partnerships, contracts, acquisitions,
   political actions, hoard/liquidate…).
2. Score each: expected profit, risk, liquidity, personality, goals,
   relationships, reputation, market conditions, memory, opportunity cost,
   time horizon, legal/political consequences. **Utility scores are the one
   sanctioned float zone** (CLAUDE.md) — they order choices; every executed
   consequence goes back through integer ledgers. Ties break by action enum
   order, then target id.
3. Store a `DecisionRecord`: actions considered, utility per action, inputs
   that mattered (memories, relationships, assumptions), chosen action,
   outcome, and whether the outcome updated behavior — surfaced verbatim in
   the agent inspector ("why did you do that?").

### Memory (Phase 2)

Bounded per-agent store: event, participants, tick, importance, emotional
impact, trust impact, financial impact, confidence, decay rate, relevance,
tags. Decay each memory-phase tick unless reinforced; repetition strengthens
derived beliefs. Deterministic inaccuracy allowed only through explicit
confidence degradation rules. Ring-bounded (importance-weighted eviction).

### Relationships & reputation (Phase 2)

Two separate systems: private dyadic relationships (trust, affection, fear,
respect, resentment, dependence, commercial reliability — integer scales,
bounded updates from interaction events) and public reputation (reliable,
honest, competent, generous, ruthless, wealthy, dangerous, influential,
corrupt) spread through observation, contract performance, business
outcomes, and rumor channels. An agent may privately trust someone the town
despises. `probe_reputation` guards the propagation channel.

### Interfaces already reserved

- Phase 9 of the tick order is the decisions slot; Phase 10 is
  memory/relationships.
- Per-agent RNG: `rng::substream(seed, "decisions", agent_id, tick)`.
- `Journal` accepts new event variants without touching hashes.
- The agent inspector consumes on-demand detail queries (protocol slot
  reserved in ARCHITECTURE.md).
