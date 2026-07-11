import { formatTickLabel } from '../format';
import type { EventRow } from '../types';

export function EventLog({ events }: { events: EventRow[] }) {
  const newestFirst = [...events].reverse();
  return (
    <div className="events">
      {newestFirst.map((e) => (
        <div className="event-row" key={e.seq}>
          <span className="event-tick">{formatTickLabel(e.tick)}</span>
          <span className={`event-kind k-${e.kind}`} title={e.kind} />
          <span className="event-text">{e.text}</span>
        </div>
      ))}
    </div>
  );
}
