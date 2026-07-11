# Data Model

Authoritative shapes live in `crates/sim-core/src`; this document is the map.
All simulation collections are ordered (`BTreeMap` by id, `Vec` in insertion
order); iteration order is part of the determinism contract.

## Identifiers

| Type | Form | Notes |
|------|------|-------|
| `AgentId` | `u32` newtype (`A7`) | stable, never reused |
| `BusinessId` | `u32` newtype (`B2`) | stable, never reused |
| `AccountId` | `Business(BusinessId) \| Agent(AgentId)` | derived `Ord` puts businesses before agents — the market buyer tie-break (DECISIONS.md #008) |

## Money & goods

- `Money(i64)` — cents. Ops: add/sub/neg/sum, `mul_bp` (i128 intermediate,
  toward-zero), `checked_mul_qty`, `affordable_units`.
- `Good` — `Wheat | Flour | Food` (Phase 1 adds ore, steel, tools, wood,
  bricks…). `Good::ALL` fixes the market clearing order.
- `Qty = i64` — whole units, invariant-checked non-negative in inventories.

## Agent (Phase 0 fields)

```
id, name
cash: Money            pantry: Qty (units of Food at home)
employer: Option<BusinessId>   owns: Option<BusinessId>
hungry_streak: u32     days_unemployed: u32
total_earned / total_spent: Money   (lifetime, UI only)
```

Traits, goals, memory, relationships, reputation: Phase 2 (see
AGENT_DESIGN.md).

## Business

```
id, name, kind (Farm|Mill|Bakery), owner: AgentId
cash: Money
workers: Vec<AgentId>          (hire order; LIFO firing)
target_headcount: u32          wage: Money (per day)
inventory: BTreeMap<Good, Qty> (inputs and outputs)
sells: Good                    price: Money (posted unit price)
recipe: { inputs: Vec<(Good, Qty)>, output: (Good, Qty), batches_per_worker }
-- rolling stats (hashed; decisions read them) --
sales_ema_milli: i64           stockout_days, vacancy_days, missed_payroll_days: u32
revenue_window, costs_window, last_window_profit: Money
-- per-day scratch --
sold_today, produced_today: Qty
```

## World = SimState + InputLog + Journal

```
SimState  { tick, config: WorldConfig, expected_total_money,
            agents: BTreeMap<AgentId, Agent>,
            businesses: BTreeMap<BusinessId, Business>,
            market: { last_prices: BTreeMap<Good, Money> },
            status: Running | Halted{reason} }          — hashed
InputLog  { command_log: Vec<QueuedCommand>, pending: Vec<QueuedCommand>,
            next_seq }                                   — saved, not hashed
Journal   { events: VecDeque<EventRecord> (cap 50k), next_event_seq,
            transactions: VecDeque<Transaction> (cap 10k), next_tx_seq,
            metrics: VecDeque<MetricsDay> (cap 4k),
            manifest: Vec<(tick, blake3 hex)> }          — saved, not hashed
```

`WorldConfig { master_seed, population, hash_every }` — the complete input to
world generation; saved in meta for replay.

## Commands, transactions, events

- `PlayerCommand::AdjustMoneySupply { account, delta, memo }` — Phase 0's
  only command; the explicit money source/sink. `QueuedCommand { seq, tick,
  command }`.
- `Transaction { seq, tick, from: Option<AccountId>, to: Option<AccountId>,
  amount, kind }` with `TxKind = Wage | GoodsPurchase{good, qty, unit_price}
  | Dividend | MonetaryPolicy{memo}`. `None` sides are mint/burn.
- `Event` (11 variants Phase 0): WorldCreated, Hired, Fired, QuitUnpaid,
  MissedPayroll, PriceChanged, WageChanged, DividendPaid, AgentHungry,
  MonetaryPolicy, CommandRejected. `EventRecord { seq, tick, event }`.

## MetricsDay (per tick)

tick, money totals (all/household/business), employed/unemployed/owners,
hungry, per-good volume-weighted avg price + volume + unmet demand, food
produced/consumed.

## Save schema (SQLite, `schema_version = 1`)

```sql
CREATE TABLE meta     (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- keys: schema_version, tick, master_seed, config (JSON), world_hash, app_version
CREATE TABLE world    (id INTEGER PRIMARY KEY CHECK (id = 1), data BLOB NOT NULL);
-- postcard blob of World
CREATE TABLE commands (seq INTEGER PRIMARY KEY, tick INTEGER NOT NULL,
                       applied INTEGER NOT NULL, data TEXT NOT NULL);   -- JSON
CREATE TABLE events   (seq INTEGER PRIMARY KEY, tick INTEGER NOT NULL,
                       kind TEXT NOT NULL, data TEXT NOT NULL);          -- JSON
CREATE TABLE manifest (tick INTEGER PRIMARY KEY, hash TEXT NOT NULL);
```

Versioning: pre-1.0, breaking save changes are allowed but noted in
PROGRESS.md. Loads refuse `schema_version` above the supported one.

## UI snapshot (`WorldSnapshot`)

Compact render-ready summary (serde snake_case JSON): tick/year/day, status,
stat chips, agent rows, business rows, 180-day price history, 120-event tail
with server-rendered text. Mirrored by `app/src/types.ts`. Never the full
world; inspector detail queries arrive in Phase 2.
