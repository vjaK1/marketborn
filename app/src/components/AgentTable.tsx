import { formatMoney } from '../format';
import type { AgentRow } from '../types';

function roleCell(a: AgentRow) {
  if (a.role === 'owner') {
    return <span className="role-owner">Owner · {a.workplace ?? '?'}</span>;
  }
  if (a.role === 'worker') {
    return <span>{a.workplace ?? 'Worker'}</span>;
  }
  return (
    <span className="role-unemployed">
      Unemployed{a.days_unemployed > 0 ? ` · ${a.days_unemployed}d` : ''}
    </span>
  );
}

export function AgentTable({ agents }: { agents: AgentRow[] }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Occupation</th>
          <th className="num">Cash</th>
          <th className="num">Pantry</th>
          <th className="num">Hunger</th>
        </tr>
      </thead>
      <tbody>
        {agents.map((a) => (
          <tr key={a.id}>
            <td>{a.name}</td>
            <td>{roleCell(a)}</td>
            <td className="num">{formatMoney(a.cash_cents)}</td>
            <td className="num">{a.pantry}</td>
            <td className="num">
              {a.hungry_streak > 0 ? (
                <span className="hungry-dot">● {a.hungry_streak}d</span>
              ) : (
                '—'
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
