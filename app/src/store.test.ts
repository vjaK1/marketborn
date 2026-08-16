import { beforeEach, describe, expect, it } from 'vitest';
import { useStore } from './store';
import type { WorldSnapshot } from './types';

function fakeSnapshot(tick: number): WorldSnapshot {
  return {
    tick,
    year: 1,
    day_of_year: tick + 1,
    status: 'running',
    stats: {
      population: 20,
      employed: 11,
      unemployed: 5,
      owners: 4,
      hungry: 0,
      money_total_cents: 108000000,
      food_price_cents: 540,
      food_stock: 30,
      gdp_week_cents: 250000,
      food_inflation_90d_bp: 120,
      cash_gini_bp: 3200,
      bank_rate_bp: 1800,
      govt_cash_cents: 0,
      govt_debt_cents: 0,
      sales_tax_bp: 100,
      welfare_floor_cents: 1200,
      minimum_wage_cents: 300,
      deficit_limit_cents: 0,
    },
    agents: [],
    businesses: [],
    markets: [],
    contracts: [],
    price_history: { ticks: [], series: [] },
    events: [],
  };
}

describe('ui store', () => {
  beforeEach(() => {
    useStore.setState({
      connected: false,
      snapshot: null,
      speed: 1,
      saveMessage: null,
    });
  });

  it('applying a snapshot marks the backend connected', () => {
    expect(useStore.getState().connected).toBe(false);
    useStore.getState().applySnapshot(fakeSnapshot(7));
    const s = useStore.getState();
    expect(s.connected).toBe(true);
    expect(s.snapshot?.tick).toBe(7);
  });

  it('newer snapshots replace older ones', () => {
    useStore.getState().applySnapshot(fakeSnapshot(1));
    useStore.getState().applySnapshot(fakeSnapshot(2));
    expect(useStore.getState().snapshot?.tick).toBe(2);
  });

  it('speed is clamped to the shell range 0..4', () => {
    useStore.getState().setSpeed(9);
    expect(useStore.getState().speed).toBe(4);
    useStore.getState().setSpeed(-3);
    expect(useStore.getState().speed).toBe(0);
    useStore.getState().setSpeed(2.9);
    expect(useStore.getState().speed).toBe(2);
  });
});
