import { useMemo, useState } from 'react';
import { formatTickLabel } from '../format';
import type { EventRow } from '../types';

/**
 * Event kinds grouped for the timeline filter (BRIEF: "timeline
 * filters"). A kind missing from every group still shows under "All" —
 * new event kinds degrade gracefully instead of vanishing.
 */
const GROUPS: { label: string; kinds: string[] }[] = [
  {
    label: 'People',
    kinds: [
      'hired',
      'fired',
      'quit_unpaid',
      'job_switched',
      'agent_hungry',
      'welfare_paid',
    ],
  },
  {
    label: 'Business',
    kinds: [
      'price_changed',
      'wage_changed',
      'dividend_paid',
      'owner_invested',
      'business_sold',
      'missed_payroll',
    ],
  },
  {
    label: 'Contracts',
    kinds: [
      'contract_signed',
      'contract_delivered',
      'contract_missed',
      'contract_breached',
      'contract_terminated',
      'contract_completed',
    ],
  },
  {
    label: 'Finance',
    kinds: [
      'loan_issued',
      'loan_payment_missed',
      'loan_repaid',
      'loan_defaulted',
      'collateral_seized',
      'bank_rate_set',
    ],
  },
  {
    label: 'Government',
    kinds: [
      'sales_tax_set',
      'welfare_floor_set',
      'minimum_wage_set',
      'deficit_limit_set',
      'gov_borrowed',
      'gov_debt_cleared',
      'monetary_policy',
      'shock_began',
      'shock_ended',
    ],
  },
];

export function EventLog({ events }: { events: EventRow[] }) {
  const [group, setGroup] = useState<string | null>(null);
  const [needle, setNeedle] = useState('');

  const visible = useMemo(() => {
    const kinds = group
      ? new Set(GROUPS.find((g) => g.label === group)?.kinds ?? [])
      : null;
    const q = needle.trim().toLowerCase();
    return [...events]
      .reverse()
      .filter((e) => (kinds ? kinds.has(e.kind) : true))
      .filter((e) => (q === '' ? true : e.text.toLowerCase().includes(q)));
  }, [events, group, needle]);

  return (
    <div className="events">
      <div className="event-filters">
        <button
          type="button"
          className={group === null ? 'active' : ''}
          onClick={() => setGroup(null)}
        >
          All
        </button>
        {GROUPS.map((g) => (
          <button
            type="button"
            key={g.label}
            className={group === g.label ? 'active' : ''}
            onClick={() => setGroup(group === g.label ? null : g.label)}
          >
            {g.label}
          </button>
        ))}
        <input
          type="text"
          placeholder="filter by name…"
          value={needle}
          onChange={(e) => setNeedle(e.target.value)}
        />
      </div>
      <div className="event-rows">
        {visible.map((e) => (
          <div className="event-row" key={e.seq}>
            <span className="event-tick">{formatTickLabel(e.tick)}</span>
            <span className={`event-kind k-${e.kind}`} title={e.kind} />
            <span className="event-text">{e.text}</span>
          </div>
        ))}
        {visible.length === 0 && (
          <div className="event-empty">nothing matches this filter</div>
        )}
      </div>
    </div>
  );
}
