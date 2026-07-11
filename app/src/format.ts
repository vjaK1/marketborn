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
