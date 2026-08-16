/**
 * The policy levers (Phase 4 commands, Phase 5 surface): each row shows a
 * lever's current value from the snapshot and enacts a change through
 * `queueCommand` — applied at the next tick boundary, like every world
 * mutation. Inputs are dollars / percent; the wire stays integer cents /
 * basis points.
 */

import { useState } from 'react';
import { formatBp, formatMoney } from '../format';
import { queueCommand } from '../ipc';
import type { Stats } from '../types';

interface LeverSpec {
  key: string;
  label: string;
  /** Current value rendered from the snapshot. */
  current: (s: Stats) => string;
  /** Placeholder documenting the input unit. */
  unit: string;
  /** Parse the input into the command, or return an error string. */
  build: (input: string) => Record<string, unknown> | string;
}

function parseDollarsToCents(input: string): number | null {
  const v = Number(input);
  if (!Number.isFinite(v) || v < 0) return null;
  return Math.round(v * 100);
}

function parsePercentToBp(input: string): number | null {
  const v = Number(input);
  if (!Number.isFinite(v) || v < 0) return null;
  return Math.round(v * 100);
}

const LEVERS: LeverSpec[] = [
  {
    key: 'sales_tax',
    label: 'Sales tax',
    current: (s) => formatBp(s.sales_tax_bp),
    unit: '%',
    build: (input) => {
      const bp = parsePercentToBp(input);
      return bp === null
        ? 'enter a percentage ≥ 0'
        : { SetSalesTax: { rate_bp: bp } };
    },
  },
  {
    key: 'bank_rate',
    label: 'Bank rate (new loans)',
    current: (s) => `${formatBp(s.bank_rate_bp)}/yr`,
    unit: '%/yr',
    build: (input) => {
      const bp = parsePercentToBp(input);
      return bp === null
        ? 'enter a percentage ≥ 0'
        : { SetBankRate: { rate_bp: bp } };
    },
  },
  {
    key: 'welfare_floor',
    label: 'Welfare floor',
    current: (s) => `${formatMoney(s.welfare_floor_cents)}/day`,
    unit: '$',
    build: (input) => {
      const cents = parseDollarsToCents(input);
      return cents === null
        ? 'enter a dollar amount ≥ 0'
        : { SetWelfareFloor: { floor: cents } };
    },
  },
  {
    key: 'minimum_wage',
    label: 'Minimum wage',
    current: (s) => `${formatMoney(s.minimum_wage_cents)}/day`,
    unit: '$',
    build: (input) => {
      const cents = parseDollarsToCents(input);
      return cents === null
        ? 'enter a dollar amount ≥ 0'
        : { SetMinimumWage: { wage: cents } };
    },
  },
  {
    key: 'deficit_limit',
    label: 'Deficit limit',
    current: (s) => formatMoney(s.deficit_limit_cents),
    unit: '$',
    build: (input) => {
      const cents = parseDollarsToCents(input);
      return cents === null
        ? 'enter a dollar amount ≥ 0'
        : { SetDeficitLimit: { limit: cents } };
    },
  },
];

export function PolicyPanel({ stats }: { stats: Stats }) {
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [notes, setNotes] = useState<Record<string, string>>({});

  const enact = async (lever: LeverSpec) => {
    const input = (drafts[lever.key] ?? '').trim();
    if (input === '') return;
    const note = (text: string) =>
      setNotes((n) => ({ ...n, [lever.key]: text }));
    const built = lever.build(input);
    if (typeof built === 'string') {
      note(built);
      return;
    }
    try {
      const { tick } = await queueCommand(built);
      note(`enacted — takes effect day ${tick}`);
      setDrafts((d) => ({ ...d, [lever.key]: '' }));
    } catch (e) {
      note(e instanceof Error ? e.message : 'command failed');
    }
  };

  return (
    <div className="policy">
      <div className="policy-budget">
        <span>
          Treasury <strong>{formatMoney(stats.govt_cash_cents)}</strong>
        </span>
        <span>
          Sovereign debt <strong>{formatMoney(stats.govt_debt_cents)}</strong>
        </span>
      </div>
      <table className="policy-table">
        <tbody>
          {LEVERS.map((lever) => (
            <tr key={lever.key}>
              <td className="policy-label">{lever.label}</td>
              <td className="policy-current">{lever.current(stats)}</td>
              <td className="policy-input">
                <input
                  type="text"
                  inputMode="decimal"
                  placeholder={lever.unit}
                  value={drafts[lever.key] ?? ''}
                  onChange={(e) =>
                    setDrafts((d) => ({ ...d, [lever.key]: e.target.value }))
                  }
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void enact(lever);
                  }}
                />
              </td>
              <td>
                <button type="button" onClick={() => void enact(lever)}>
                  Enact
                </button>
              </td>
              <td className="policy-note">{notes[lever.key] ?? ''}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
