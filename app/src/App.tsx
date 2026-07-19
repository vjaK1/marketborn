import { useEffect, useState } from 'react';
import { AgentTable } from './components/AgentTable';
import { BusinessTable } from './components/BusinessTable';
import { EventLog } from './components/EventLog';
import { MarketTable } from './components/MarketTable';
import { PriceChart } from './components/PriceChart';
import { SpeedControls } from './components/SpeedControls';
import { StatsBar } from './components/StatsBar';
import { initIpc, isTauri } from './ipc';
import { useStore } from './store';

export default function App() {
  const snapshot = useStore((s) => s.snapshot);
  const applySnapshot = useStore((s) => s.applySnapshot);
  const [backend, setBackend] = useState<'connecting' | 'browser' | 'ready'>(
    'connecting',
  );

  useEffect(() => {
    let cancelled = false;
    void initIpc((snap) => {
      if (!cancelled) applySnapshot(snap);
    }).then((ok) => {
      if (!cancelled) setBackend(ok ? 'ready' : 'browser');
    });
    return () => {
      cancelled = true;
    };
  }, [applySnapshot]);

  if (!snapshot) {
    return (
      <div className="waiting">
        <div className="brand">
          Market<span>born</span>
        </div>
        {backend === 'browser' && !isTauri() ? (
          <p>
            No simulation backend. Launch the desktop app
            (<code>npm run app:desktop</code>) — this page only renders the UI
            shell in a plain browser.
          </p>
        ) : (
          <p>Waiting for the simulation…</p>
        )}
      </div>
    );
  }

  const halted = snapshot.status !== 'running';
  return (
    <div className="app">
      <header className="header">
        <div className="brand">
          Market<span>born</span>
        </div>
        <div className="date">
          Y{snapshot.year} · D{snapshot.day_of_year}
        </div>
        {halted && <div className="status-halted">{snapshot.status}</div>}
        <div className="spacer" />
        <SpeedControls />
      </header>
      <StatsBar stats={snapshot.stats} />
      <div className="grid">
        <section className="panel">
          <h2>Prices — daily average</h2>
          <div className="body">
            <PriceChart history={snapshot.price_history} />
          </div>
        </section>
        <div className="stack">
          <section className="panel">
            <h2>Businesses</h2>
            <div className="body">
              <BusinessTable businesses={snapshot.businesses} />
            </div>
          </section>
          <section className="panel">
            <h2>Markets</h2>
            <div className="body">
              <MarketTable markets={snapshot.markets} />
            </div>
          </section>
        </div>
        <section className="panel">
          <h2>Event log</h2>
          <div className="body">
            <EventLog events={snapshot.events} />
          </div>
        </section>
        <section className="panel">
          <h2>Agents</h2>
          <div className="body">
            <AgentTable agents={snapshot.agents} />
          </div>
        </section>
      </div>
    </div>
  );
}
