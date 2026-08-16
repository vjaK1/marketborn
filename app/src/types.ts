/**
 * TypeScript mirror of the Rust `WorldSnapshot` protocol
 * (crates/sim-core/src/snapshot.rs). Field names match the serde output.
 */

export interface WorldSnapshot {
  tick: number;
  year: number;
  day_of_year: number;
  status: string;
  stats: Stats;
  agents: AgentRow[];
  businesses: BusinessRow[];
  markets: MarketRow[];
  contracts: ContractRow[];
  price_history: PriceHistory;
  macro_history: MacroHistory;
  events: EventRow[];
}

export interface MacroHistory {
  ticks: number[];
  employed: number[];
  hungry: number[];
  govt_cash_cents: number[];
  govt_debt_cents: number[];
}

export interface ContractRow {
  id: number;
  seller: string;
  buyer: string;
  good: string;
  qty: number;
  unit_price_cents: number;
  state: string;
  delivered: number;
  missed: number;
  deliveries: number;
  start_tick: number;
}

export interface ContractDetail {
  id: number;
  seller: string;
  buyer: string;
  good: string;
  qty: number;
  unit_price_cents: number;
  state: string;
  start_tick: number;
  next_due: number;
  deliveries: number;
  delivered: number;
  missed: number;
  delivered_units: number;
  paid_total_cents: number;
  penalties_cents: number;
  negotiation: NegotiationRow[];
  history: TickText[];
}

export interface NegotiationRow {
  by: string;
  price_cents: number;
  because: string;
}

export interface BusinessDetail {
  id: number;
  name: string;
  kind: string;
  owner: string;
  owner_id: number;
  sells: string;
  price_cents: number;
  wage_cents: number;
  workers: string[];
  target_headcount: number;
  expected_daily_sales: number;
  stockout_days: number;
  last_window_profit_cents: number;
  lifetime_profit_cents: number;
  inventory: { good: string; qty: number }[];
  cash_cents: number;
  inventory_value_cents: number;
  assets_cents: number;
  liabilities_cents: number;
  equity_cents: number;
  books: { name: string; cents: number }[];
  spoiled_units: number;
  seized_units: number;
  loan: {
    id: number;
    principal_cents: number;
    outstanding_cents: number;
    rate_bp: number;
    missed_payments: number;
    start_tick: number;
  } | null;
  prior_defaults: number;
  contracts: {
    id: number;
    role: string;
    counterparty: string;
    good: string;
    qty: number;
    unit_price_cents: number;
    state: string;
    delivered: number;
    deliveries: number;
  }[];
  history: { tick: number; text: string }[];
}

export interface MarketRow {
  good: string;
  last_price_cents: number | null;
  volume_today: number;
  unmet_today: number;
  spoiled_today: number;
  sellers: number;
  offered_qty: number;
  best_ask_cents: number | null;
  demand_qty: number;
  urgent_demand_qty: number;
  world_stock: number;
}

export interface Stats {
  population: number;
  employed: number;
  unemployed: number;
  owners: number;
  hungry: number;
  money_total_cents: number;
  food_price_cents: number | null;
  food_stock: number;
  gdp_week_cents: number;
  food_inflation_90d_bp: number | null;
  cash_gini_bp: number;
  bank_rate_bp: number;
  govt_cash_cents: number;
  govt_debt_cents: number;
  sales_tax_bp: number;
  welfare_floor_cents: number;
  minimum_wage_cents: number;
  deficit_limit_cents: number;
}

export interface AgentRow {
  id: number;
  name: string;
  role: string;
  workplace: string | null;
  cash_cents: number;
  pantry: number;
  owns_home: boolean;
  hungry_streak: number;
  days_unemployed: number;
}

export interface BusinessRow {
  id: number;
  name: string;
  kind: string;
  cash_cents: number;
  workers: number;
  target_workers: number;
  wage_cents: number;
  sells: string;
  price_cents: number;
  output_stock: number;
  input_stock: InputStockRow[];
  last_window_profit_cents: number;
  sold_today: number;
  produced_today: number;
  books: BooksRow;
}

export interface BooksRow {
  revenue_cents: number;
  input_costs_cents: number;
  tool_costs_cents: number;
  wages_cents: number;
  dividends_cents: number;
  owner_invested_cents: number;
  lifetime_profit_cents: number;
  spoiled_units: number;
  inventory_value_cents: number;
  assets_cents: number;
}

export interface InputStockRow {
  good: string;
  qty: number;
}

export interface PriceHistory {
  ticks: number[];
  series: GoodSeries[];
}

export interface GoodSeries {
  good: string;
  points: (number | null)[];
}

export interface EventRow {
  seq: number;
  tick: number;
  kind: string;
  text: string;
}

/** On-demand agent inspector payload (crates/sim-core/src/inspect.rs). */
export interface AgentDetail {
  id: number;
  name: string;
  role: string;
  workplace: string | null;
  cash_cents: number;
  pantry: number;
  owns_home: boolean;
  hungry_streak: number;
  days_unemployed: number;
  total_earned_cents: number;
  total_spent_cents: number;
  traits: NamedValue[];
  memories: TickText[];
  relations: RelationRow[];
  beliefs: BeliefRow[];
  decisions: TickText[];
}

export interface NamedValue {
  name: string;
  value: number;
}

export interface TickText {
  tick: number;
  text: string;
}

export interface RelationRow {
  toward: string;
  trust: number;
  affection: number;
  fear: number;
  respect: number;
  resentment: number;
  dependence: number;
  commercial_reliability: number;
}

export interface BeliefRow {
  about: string;
  reliable: number;
  generous: number;
  ruthless: number;
}
