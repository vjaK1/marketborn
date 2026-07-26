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
override; **entry/exit v1** — demand-gated takeover-revival of moribund
businesses by wealthy entrepreneurs (ambition + risk tolerance), with
equity sales through the ledger and same-tick recapitalization;
**memory v1** (#023) — the bounded, decaying, reinforced grievance store
with weakest-first eviction, consumed by hiring/switch targeting
(non-desperate agents refuse employers they remember being stiffed or
fired by); **relationships v1** (#024) — all seven private dimensions
with live drivers at interaction sites, weekly drift toward neutral, and
the bond-adjusted switch premium (attachment binds, resentment repels);
**reputation v1** (#025) — belief-based, propagated by workplace +
neighborhood gossip with "neutrality is silence", consumed by hiring
refusal of publicly-unreliable owners; `probe_reputation` **passes**.
Seeds produce economically distinct histories; the **agent inspector**
ships (`inspect.rs` + on-demand detail protocol + UI panel) with decision
explanations verbatim — **all four Phase 2 acceptance criteria are met**.
Continuing opportunistically in later phases: remaining owner reviews on
the engine, founding genuinely new firms, and the reputation dimensions
whose drivers arrive with contracts (Phase 3) and politics (Phase 4).

## Phase 3 status (contract kernel, DECISIONS #026)

The engine gained two contract actions. **SupplyReview** (weekly, buyer's
stagger): Sign vs StaySpot over a live requirements-contract offer —
greed weighs the 5% commitment discount, caution (inverse risk
tolerance) buys supply security as input cover thins, and a commitment
cost scaling with risk tolerance squared keeps gamblers on the spot
market until a crunch converts them; both outcomes journal.
**ContractExit** (recorded only when it happens): a buyer walks away from
a contract locked above its input reservation cap once the gap exceeds an
honesty-widened tolerance (0–10% past the cap) — honest owners honor
deals longer at real cost; the exit pays a penalty and the jilted owner's
belief sours. Contract performance is the first Phase 3 reputation
driver: deliveries build private commercial reliability both ways; misses
and walk-aways seed public "unreliable" beliefs that gossip carries.

With the bank (DECISIONS #027) the engine gained **BorrowReview** — the
distress ladder's third rung (own till, owner injection, then credit):
Borrow vs Struggle, where payroll runway is the urgency, the bank's rate
the price, and debt aversion (inverse risk tolerance) the weight. A
punitive rate flips everyone but the desperate-and-bold to Struggle —
the transmission channel `probe_rate_shock` pins. The brief's "borrow"
and "repay debt" actions are live; "invest" now spans hire funding and
working capital.

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
