import { useEffect, useState } from 'react';
import { AgentInspector } from './components/AgentInspector';
import { AgentTable } from './components/AgentTable';
import { BusinessTable } from './components/BusinessTable';
import { ContractInspector } from './components/ContractInspector';
import { ContractTable } from './components/ContractTable';
import { EventLog } from './components/EventLog';
import { MarketTable } from './components/MarketTable';
import { PolicyPanel } from './components/PolicyPanel';
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
  const [inspected, setInspected] = useState<number | null>(null);
  const [inspectedContract, setInspectedContract] = useState<number | null>(
    null,
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
            (<code>npm run app:desktop</code>), or start{' '}
            <code>sim-cli serve</code> and reload — in a browser this page
            connects to <code>ws://127.0.0.1:17771</code> (override with{' '}
            <code>?ws=</code>).
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
          <section className="panel">
            <h2>Government</h2>
            <div className="body">
              <PolicyPanel stats={snapshot.stats} />
            </div>
          </section>
          <section className="panel">
            <h2>
              {inspectedContract === null ? 'Contracts' : 'Contract inspector'}
            </h2>
            <div className="body">
              {inspectedContract === null ? (
                <ContractTable
                  contracts={snapshot.contracts}
                  onSelect={setInspectedContract}
                />
              ) : (
                <ContractInspector
                  id={inspectedContract}
                  onBack={() => setInspectedContract(null)}
                />
              )}
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
          <h2>{inspected === null ? 'Agents' : 'Agent inspector'}</h2>
          <div className="body">
            {inspected === null ? (
              <AgentTable agents={snapshot.agents} onSelect={setInspected} />
            ) : (
              <AgentInspector id={inspected} onBack={() => setInspected(null)} />
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
