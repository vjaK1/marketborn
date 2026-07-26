import { useEffect, useState } from 'react';
import { formatMoney, formatTickLabel } from '../format';
import { getContractDetail } from '../ipc';
import type { ContractDetail } from '../types';

/**
 * The contract inspector (Phase 3): terms and tallies, the complete
 * negotiation log — every offer, counteroffer and the reason for each
 * move — and the delivery/miss/breach history. Fetched on demand and
 * refreshed once a second while open.
 */
export function ContractInspector({
  id,
  onBack,
}: {
  id: number;
  onBack: () => void;
}) {
  const [detail, setDetail] = useState<ContractDetail | null>(null);

  useEffect(() => {
    let cancelled = false;
    const fetch = () => {
      void getContractDetail(id).then((d) => {
        if (!cancelled && d) setDetail(d);
      });
    };
    fetch();
    const timer = setInterval(fetch, 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [id]);

  if (!detail) {
    return (
      <div className="inspector">
        <button className="back-link" onClick={onBack}>
          ← All contracts
        </button>
        <p className="muted-note">Fetching…</p>
      </div>
    );
  }

  return (
    <div className="inspector">
      <button className="back-link" onClick={onBack}>
        ← All contracts
      </button>
      <div className="inspector-head">
        <span className="inspector-name">
          {detail.seller} → {detail.buyer}
        </span>
        <span className="inspector-role">
          up to {detail.qty} {detail.good}/day at{' '}
          {formatMoney(detail.unit_price_cents)} ·{' '}
          <span className={`contract-state contract-${detail.state}`}>
            {detail.state}
          </span>
        </span>
      </div>
      <div className="inspector-stats">
        <span>signed {formatTickLabel(detail.start_tick)}</span>
        <span>
          delivered {detail.delivered}/{detail.deliveries} days
        </span>
        <span>{detail.delivered_units} units</span>
        <span>missed {detail.missed}</span>
        <span>paid {formatMoney(detail.paid_total_cents)}</span>
        {detail.penalties_cents > 0 && (
          <span className="hungry-dot">
            penalties {formatMoney(detail.penalties_cents)}
          </span>
        )}
      </div>

      <h3>Negotiation</h3>
      {detail.negotiation.length === 0 ? (
        <p className="muted-note">
          The table talk has scrolled out of the record.
        </p>
      ) : (
        <ul className="inspector-list">
          {detail.negotiation.map((r, i) => (
            <li key={i}>
              <b>{r.by}</b> {r.because} — {formatMoney(r.price_cents)}
            </li>
          ))}
        </ul>
      )}

      <h3>History</h3>
      {detail.history.length === 0 ? (
        <p className="muted-note">Nothing has happened yet.</p>
      ) : (
        <ul className="inspector-list">
          {detail.history.map((h, i) => (
            <li key={i}>
              <span className="tick-tag">{formatTickLabel(h.tick)}</span>{' '}
              {h.text}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
