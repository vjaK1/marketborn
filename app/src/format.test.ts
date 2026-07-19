import { describe, expect, it } from 'vitest';
import { formatMoney, formatMoneyWhole, formatTickLabel, goodLabel, tickToDate } from './format';

describe('formatMoney', () => {
  it('formats cents as dollars with two decimals', () => {
    expect(formatMoney(0)).toBe('$0.00');
    expect(formatMoney(5)).toBe('$0.05');
    expect(formatMoney(123456)).toBe('$1,234.56');
    expect(formatMoney(90000)).toBe('$900.00');
  });

  it('handles negatives with a leading sign', () => {
    expect(formatMoney(-5)).toBe('-$0.05');
    expect(formatMoney(-123456)).toBe('-$1,234.56');
  });

  it('separates thousands', () => {
    expect(formatMoney(108000000)).toBe('$1,080,000.00');
  });
});

describe('formatMoneyWhole', () => {
  it('drops cents and keeps separators', () => {
    expect(formatMoneyWhole(108000099)).toBe('$1,080,000');
    expect(formatMoneyWhole(-250)).toBe('-$2');
  });
});

describe('tickToDate', () => {
  it('maps ticks onto a 360-day calendar starting at Y1 D1', () => {
    expect(tickToDate(0)).toEqual({ year: 1, day: 1 });
    expect(tickToDate(359)).toEqual({ year: 1, day: 360 });
    expect(tickToDate(360)).toEqual({ year: 2, day: 1 });
    expect(tickToDate(3650)).toEqual({ year: 11, day: 51 });
  });

  it('clamps negatives to the epoch', () => {
    expect(tickToDate(-5)).toEqual({ year: 1, day: 1 });
  });

  it('renders compact labels', () => {
    expect(formatTickLabel(360)).toBe('Y2 · D1');
  });
});

describe('goodLabel', () => {
  it('capitalizes good names', () => {
    expect(goodLabel('wheat')).toBe('Wheat');
    expect(goodLabel('iron ore')).toBe('Iron ore');
    expect(goodLabel('')).toBe('');
  });
});
