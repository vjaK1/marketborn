import { formatMoney, goodLabel } from '../format';
import type { MarketRow } from '../types';

/**
 * Market view v1: per-good standing depth (sellers, offered quantity, best
 * ask, demand) derived server-side from the live order rules, plus the last
 * completed day's outcomes (volume, unmet demand, spoilage).
 */
export function MarketTable({ markets }: { markets: MarketRow[] }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Good</th>
          <th className="num">Price</th>
          <th className="num">Vol</th>
          <th className="num">Unmet</th>
          <th className="num">Rot</th>
          <th className="num">Best ask</th>
          <th className="num">Offered</th>
          <th className="num">Demand</th>
          <th className="num">Stock</th>
        </tr>
      </thead>
      <tbody>
        {markets.map((m) => {
          const shortage = m.unmet_today > 0 || m.demand_qty > m.offered_qty;
          return (
            <tr key={m.good}>
              <td>{goodLabel(m.good)}</td>
              <td className="num">
                {m.last_price_cents !== null ? formatMoney(m.last_price_cents) : '—'}
              </td>
              <td className="num">{m.volume_today}</td>
              <td className={`num${m.unmet_today > 0 ? ' neg' : ''}`}>{m.unmet_today}</td>
              <td className="num">{m.spoiled_today > 0 ? m.spoiled_today : '—'}</td>
              <td className="num">
                {m.best_ask_cents !== null ? formatMoney(m.best_ask_cents) : '—'}
              </td>
              <td className="num" title={`${m.sellers} seller(s)`}>
                {m.offered_qty}
              </td>
              <td
                className={`num${shortage ? ' neg' : ''}`}
                title={m.urgent_demand_qty > 0 ? `${m.urgent_demand_qty} urgent` : undefined}
              >
                {m.demand_qty}
              </td>
              <td className="num">{m.world_stock}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
