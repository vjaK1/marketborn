import { formatMoney, goodLabel } from '../format';
import type { BusinessRow } from '../types';

export function BusinessTable({ businesses }: { businesses: BusinessRow[] }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Business</th>
          <th className="num">Staff</th>
          <th className="num">Wage/d</th>
          <th>Sells</th>
          <th className="num">Price</th>
          <th className="num">Stock</th>
          <th className="num">Sold today</th>
          <th className="num">Profit (7d)</th>
          <th className="num">Cash</th>
          <th className="num">Assets</th>
        </tr>
      </thead>
      <tbody>
        {businesses.map((b) => {
          const short = b.workers < b.target_workers;
          const profit = b.last_window_profit_cents;
          const lifetime = b.books.lifetime_profit_cents;
          return (
            <tr key={b.id}>
              <td title={b.kind}>{b.name}</td>
              <td className={`num${short ? ' workers-short' : ''}`}>
                {b.workers}/{b.target_workers}
              </td>
              <td className="num">{formatMoney(b.wage_cents)}</td>
              <td>{goodLabel(b.sells)}</td>
              <td className="num">{formatMoney(b.price_cents)}</td>
              <td className="num">{b.output_stock}</td>
              <td className="num">{b.sold_today}</td>
              <td className={`num ${profit > 0 ? 'pos' : profit < 0 ? 'neg' : ''}`}>
                {formatMoney(profit)}
              </td>
              <td className="num">{formatMoney(b.cash_cents)}</td>
              <td
                className="num"
                title={`Cash + inventory at market value · lifetime operating profit ${formatMoney(lifetime)}`}
              >
                {formatMoney(b.books.assets_cents)}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
