import { useEffect, useState } from 'react';
import { formatMoney, formatTickLabel } from '../format';
import { getAgentDetail } from '../ipc';
import type { AgentDetail } from '../types';

/**
 * The agent inspector (Phase 2 acceptance): identity, traits, memories,
 * private relations, public beliefs, and the agent's recent decisions with
 * the engine's explanations verbatim. Fetched on demand and refreshed once
 * a second while open — never through the 10 Hz snapshot.
 */
export function AgentInspector({ id, onBack }: { id: number; onBack: () => void }) {
  const [detail, setDetail] = useState<AgentDetail | null>(null);

  useEffect(() => {
    let cancelled = false;
    const fetch = () => {
      void getAgentDetail(id).then((d) => {
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
          ← All agents
        </button>
        <p className="muted-note">Fetching…</p>
      </div>
    );
  }

  return (
    <div className="inspector">
      <button className="back-link" onClick={onBack}>
        ← All agents
      </button>
      <div className="inspector-head">
        <span className="inspector-name">{detail.name}</span>
        <span className="inspector-role">
          {detail.role}
          {detail.workplace ? ` · ${detail.workplace}` : ''}
        </span>
      </div>
      <div className="inspector-stats">
        <span>{formatMoney(detail.cash_cents)} cash</span>
        <span>pantry {detail.pantry}</span>
        <span>{detail.owns_home ? 'homeowner' : 'no home'}</span>
        {detail.hungry_streak > 0 && (
          <span className="hungry-dot">hungry {detail.hungry_streak}d</span>
        )}
        <span>earned {formatMoney(detail.total_earned_cents)}</span>
        <span>spent {formatMoney(detail.total_spent_cents)}</span>
      </div>

      <h3>Personality</h3>
      <div className="trait-grid">
        {detail.traits.map((t) => (
          <span key={t.name} className="trait-chip" title={`${t.name}: ${t.value} / 100`}>
            {t.name} <b>{t.value}</b>
          </span>
        ))}
      </div>

      <h3>Recent decisions</h3>
      {detail.decisions.length === 0 ? (
        <p className="muted-note">No recorded decisions yet.</p>
      ) : (
        <ul className="inspector-list">
          {detail.decisions.map((d, i) => (
            <li key={i}>
              <span className="tick-tag">{formatTickLabel(d.tick)}</span> {d.text}
            </li>
          ))}
        </ul>
      )}

      <h3>Memories</h3>
      {detail.memories.length === 0 ? (
        <p className="muted-note">Nothing weighs on them.</p>
      ) : (
        <ul className="inspector-list">
          {detail.memories.map((m, i) => (
            <li key={i}>
              <span className="tick-tag">{formatTickLabel(m.tick)}</span> {m.text}
            </li>
          ))}
        </ul>
      )}

      <h3>Relationships</h3>
      {detail.relations.length === 0 ? (
        <p className="muted-note">No one close enough to have an opinion of.</p>
      ) : (
        <ul className="inspector-list">
          {detail.relations.map((r) => (
            <li key={r.toward}>
              <b>{r.toward}</b> — trust {r.trust} · affection {r.affection} · fear {r.fear} ·
              respect {r.respect} · resentment {r.resentment} · dependence {r.dependence} ·
              reliability {r.commercial_reliability}
            </li>
          ))}
        </ul>
      )}

      <h3>Beliefs about others</h3>
      {detail.beliefs.length === 0 ? (
        <p className="muted-note">No rumors worth repeating.</p>
      ) : (
        <ul className="inspector-list">
          {detail.beliefs.map((b) => (
            <li key={b.about}>
              <b>{b.about}</b> — reliable {b.reliable} · generous {b.generous} · ruthless{' '}
              {b.ruthless}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
