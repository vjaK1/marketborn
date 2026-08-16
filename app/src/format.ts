/** Display formatting. Money stays integer cents until the final string. */

export function formatMoney(cents: number): string {
  const sign = cents < 0 ? '-' : '';
  const abs = Math.abs(Math.trunc(cents));
  const dollars = Math.floor(abs / 100);
  const remainder = abs % 100;
  return `${sign}$${dollars.toLocaleString('en-US')}.${remainder
    .toString()
    .padStart(2, '0')}`;
}

/** Compact form for dense chips: whole dollars only. */
export function formatMoneyWhole(cents: number): string {
  const sign = cents < 0 ? '-' : '';
  const dollars = Math.floor(Math.abs(Math.trunc(cents)) / 100);
  return `${sign}$${dollars.toLocaleString('en-US')}`;
}

export const DAYS_PER_YEAR = 360;

export function tickToDate(tick: number): { year: number; day: number } {
  const t = Math.max(0, Math.trunc(tick));
  return { year: Math.floor(t / DAYS_PER_YEAR) + 1, day: (t % DAYS_PER_YEAR) + 1 };
}

export function formatTickLabel(tick: number): string {
  const { year, day } = tickToDate(tick);
  return `Y${year} · D${day}`;
}

/** Good name with an uppercase first letter, for legends and labels. */
export function goodLabel(good: string): string {
  return good.length === 0 ? good : good[0]!.toUpperCase() + good.slice(1);
}

/** Basis points as a percentage string, e.g. 1800 → "18%", 125 → "1.25%". */
export function formatBp(bp: number): string {
  const pct = bp / 100;
  const rounded = Math.round(pct * 100) / 100;
  return `${rounded.toLocaleString('en-US', { maximumFractionDigits: 2 })}%`;
}

/** Signed basis points for change readouts, e.g. −250 → "−2.5%". */
export function formatBpSigned(bp: number): string {
  const sign = bp > 0 ? '+' : '';
  return `${sign}${formatBp(bp)}`;
}
