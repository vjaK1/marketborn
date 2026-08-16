import { formatBp, formatBpSigned, formatMoney, formatMoneyWhole } from '../format';
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
    { label: 'GDP (7d)', value: formatMoneyWhole(stats.gdp_week_cents) },
    {
      label: 'Food inflation (90d)',
      value:
        stats.food_inflation_90d_bp !== null
          ? formatBpSigned(stats.food_inflation_90d_bp)
          : '—',
      tone:
        stats.food_inflation_90d_bp !== null && stats.food_inflation_90d_bp > 1_000
          ? 'warn'
          : undefined,
    },
    { label: 'Cash Gini', value: formatBp(stats.cash_gini_bp) },
    {
      label: 'Food price',
      value:
        stats.food_price_cents !== null
          ? formatMoney(stats.food_price_cents)
          : '—',
    },
    { label: 'Food on shelves', value: String(stats.food_stock) },
    { label: 'Bank rate', value: `${formatBp(stats.bank_rate_bp)}/yr` },
    {
      label: 'Treasury',
      value: formatMoney(stats.govt_cash_cents),
    },
    {
      label: 'Govt debt',
      value: formatMoney(stats.govt_debt_cents),
      tone: stats.govt_debt_cents > 0 ? 'warn' : undefined,
    },
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
