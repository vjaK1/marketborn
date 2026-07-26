import { formatMoney, formatTickLabel } from '../format';
import type { ContractRow } from '../types';

/**
 * The contract view's table (Phase 3): every supply contract the snapshot
 * carries (newest first, live and terminal alike). Click a row for the
 * full record — terms, the complete negotiation log, and the delivery
 * history.
 */
export function ContractTable({
  contracts,
  onSelect,
}: {
  contracts: ContractRow[];
  onSelect: (id: number) => void;
}) {
  if (contracts.length === 0) {
    return <p className="muted-note">No contracts signed yet.</p>;
  }
  return (
    <table>
      <thead>
        <tr>
          <th>Supplier</th>
          <th>Buyer</th>
          <th>Good</th>
          <th className="num">Up to/day</th>
          <th className="num">Price</th>
          <th className="num">Delivered</th>
          <th className="num">Missed</th>
          <th>Signed</th>
          <th>State</th>
        </tr>
      </thead>
      <tbody>
        {contracts.map((c) => (
          <tr key={c.id} className="clickable" onClick={() => onSelect(c.id)}>
            <td>{c.seller}</td>
            <td>{c.buyer}</td>
            <td>{c.good}</td>
            <td className="num">{c.qty}</td>
            <td className="num">{formatMoney(c.unit_price_cents)}</td>
            <td className="num">
              {c.delivered}/{c.deliveries}
            </td>
            <td className="num">{c.missed}</td>
            <td>{formatTickLabel(c.start_tick)}</td>
            <td>
              <span className={`contract-state contract-${c.state}`}>{c.state}</span>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
