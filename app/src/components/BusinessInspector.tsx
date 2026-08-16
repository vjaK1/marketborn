import { useEffect, useState } from 'react';
import { formatBp, formatMoney, formatTickLabel } from '../format';
import { getBusinessDetail } from '../ipc';
import type { BusinessDetail } from '../types';

/**
 * The business inspector (BRIEF v1.0 screen): identity and staffing, the
 * lifetime cash-basis books (the same categories the `business_books`
 * invariant reconciles every sweep), a balance sheet at market valuation,
 * credit standing, contracts on both sides, and the recent event history.
 * Fetched on demand and refreshed once a second while open.
 */
export function BusinessInspector({
  id,
  onBack,
}: {
  id: number;
  onBack: () => void;
}) {
  const [detail, setDetail] = useState<BusinessDetail | null>(null);

  useEffect(() => {
    let cancelled = false;
    const fetch = () => {
      void getBusinessDetail(id).then((d) => {
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
          ← All businesses
        </button>
        <p className="muted-note">Fetching…</p>
      </div>
    );
  }

  const staffed = `${detail.workers.length}/${detail.target_headcount}`;
  return (
    <div className="inspector">
      <button className="back-link" onClick={onBack}>
        ← All businesses
      </button>
      <div className="inspector-head">
        <span className="inspector-name">{detail.name}</span>
        <span className="inspector-role">
          {detail.kind} · owned by {detail.owner}
        </span>
      </div>
      <div className="inspector-stats">
        <span>
          sells {detail.sells} at {formatMoney(detail.price_cents)}
        </span>
        <span>
          {staffed} staff at {formatMoney(detail.wage_cents)}/day
        </span>
        <span>~{detail.expected_daily_sales}/day expected</span>
        <span
          className={
            detail.last_window_profit_cents < 0 ? 'hungry-dot' : undefined
          }
        >
          week {formatMoney(detail.last_window_profit_cents)}
        </span>
        <span>lifetime {formatMoney(detail.lifetime_profit_cents)}</span>
      </div>

      <h3>Balance sheet</h3>
      <ul className="inspector-list">
        <li>
          cash {formatMoney(detail.cash_cents)} · inventory{' '}
          {formatMoney(detail.inventory_value_cents)} at market ·{' '}
          <b>assets {formatMoney(detail.assets_cents)}</b>
        </li>
        <li>
          debt {formatMoney(detail.liabilities_cents)} ·{' '}
          <b>equity {formatMoney(detail.equity_cents)}</b>
        </li>
        {detail.inventory.length > 0 && (
          <li>
            on hand:{' '}
            {detail.inventory.map((r) => `${r.qty} ${r.good}`).join(' · ')}
          </li>
        )}
        {(detail.spoiled_units > 0 || detail.seized_units > 0) && (
          <li className="muted-note">
            lifetime write-downs: {detail.spoiled_units} spoiled ·{' '}
            {detail.seized_units} seized
          </li>
        )}
      </ul>

      <h3>Credit</h3>
      {detail.loan ? (
        <ul className="inspector-list">
          <li>
            loan L{detail.loan.id}: {formatMoney(detail.loan.outstanding_cents)}{' '}
            outstanding of {formatMoney(detail.loan.principal_cents)} at{' '}
            {formatBp(detail.loan.rate_bp)}/yr
            {detail.loan.missed_payments > 0
              ? ` · ${detail.loan.missed_payments} missed payments`
              : ''}
          </li>
        </ul>
      ) : (
        <p className="muted-note">
          {detail.prior_defaults > 0
            ? `No active loan — ${detail.prior_defaults} prior default(s); the bank remembers.`
            : 'Debt-free.'}
        </p>
      )}

      <h3>Contracts</h3>
      {detail.contracts.length === 0 ? (
        <p className="muted-note">Spot market only.</p>
      ) : (
        <ul className="inspector-list">
          {detail.contracts.map((c) => (
            <li key={c.id}>
              C{c.id} — {c.role} of {c.qty} {c.good}/day to{' '}
              <b>{c.counterparty}</b> at {formatMoney(c.unit_price_cents)} ·{' '}
              {c.delivered}/{c.deliveries} delivered · {c.state}
            </li>
          ))}
        </ul>
      )}

      <h3>Lifetime books</h3>
      <ul className="inspector-list books-list">
        {detail.books
          .filter((r) => r.cents !== 0)
          .map((r) => (
            <li key={r.name}>
              <span>{r.name}</span>
              <b className={r.cents < 0 ? 'flow-out' : 'flow-in'}>
                {formatMoney(r.cents)}
              </b>
            </li>
          ))}
      </ul>

      <h3>Staff</h3>
      {detail.workers.length === 0 ? (
        <p className="muted-note">Nobody on the roster.</p>
      ) : (
        <p className="muted-note">{detail.workers.join(' · ')}</p>
      )}

      <h3>Recent history</h3>
      {detail.history.length === 0 ? (
        <p className="muted-note">Quiet, as far as the record reaches.</p>
      ) : (
        <ul className="inspector-list">
          {[...detail.history].reverse().map((h, i) => (
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
