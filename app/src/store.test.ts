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
    },
    agents: [],
    businesses: [],
    markets: [],
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
