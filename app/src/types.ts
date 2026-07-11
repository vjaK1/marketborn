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
  price_history: PriceHistory;
  events: EventRow[];
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
}

export interface AgentRow {
  id: number;
  name: string;
  role: string;
  workplace: string | null;
  cash_cents: number;
  pantry: number;
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
