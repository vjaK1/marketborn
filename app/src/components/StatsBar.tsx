import { formatMoney, formatMoneyWhole } from '../format';
import type { Stats } from '../types';

export function StatsBar({ stats }: { stats: Stats }) {
  const chips: { label: string; value: string; tone?: 'warn' | 'bad' }[] = [
    { label: 'Population', value: String(stats.population) },
    {
      label: 'Employed',
      value: `${stats.employed} / ${stats.employed + stats.unemployed}`,
    },
    {
      label: 'Unemployed',
      value: String(stats.unemployed),
      tone: stats.unemployed > 0 ? 'warn' : undefined,
    },
    {
      label: 'Hungry',
      value: String(stats.hungry),
      tone: stats.hungry > 0 ? 'bad' : undefined,
    },
    { label: 'Money supply', value: formatMoneyWhole(stats.money_total_cents) },
    {
      label: 'Food price',
      value:
        stats.food_price_cents !== null
          ? formatMoney(stats.food_price_cents)
          : '—',
    },
    { label: 'Food on shelves', value: String(stats.food_stock) },
  ];
  return (
    <div className="stats">
      {chips.map((c) => (
        <div className="chip" key={c.label}>
          <div className="label">{c.label}</div>
          <div className={`value${c.tone ? ` ${c.tone}` : ''}`}>{c.value}</div>
        </div>
      ))}
    </div>
  );
}
