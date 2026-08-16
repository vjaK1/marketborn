/**
 * The stylised 2D city map (BRIEF v1.0 screen): farmland, the town
 * center's civic buildings, the industry side, and a residential strip —
 * every tile derived from snapshot data with a deterministic layout by
 * kind and id. The simulation has no spatial model; this view is pure
 * presentation and must never pretend otherwise (no distances, no
 * routes — the BRIEF's "transport routes" ride a real spatial model if
 * one ever exists).
 *
 * Houses are one glyph per resident: filled when they own their home,
 * hollow when they rent, flushed red while they are hungry. Clicking a
 * house opens the agent inspector.
 */

import type { AgentRow, BusinessRow, Stats } from '../types';
import { formatBp, formatMoney } from '../format';

const TILE_W = 92;
const TILE_H = 46;
const GAP = 10;

interface Placed {
  b: BusinessRow;
  x: number;
  y: number;
}

/** Chunk `items` into rows of `perRow`, returning tile positions. */
function layout(
  items: BusinessRow[],
  x0: number,
  y0: number,
  perRow: number,
): { placed: Placed[]; bottom: number } {
  const placed = items.map((b, i) => ({
    b,
    x: x0 + (i % perRow) * (TILE_W + GAP),
    y: y0 + Math.floor(i / perRow) * (TILE_H + GAP),
  }));
  const rows = Math.ceil(items.length / perRow);
  return { placed, bottom: y0 + rows * (TILE_H + GAP) };
}

function BusinessTile({ b, x, y }: Placed) {
  const dead = b.workers === 0;
  const title = `${b.name} — ${b.kind}\n${b.workers}/${b.target_workers} workers · ${formatMoney(
    b.cash_cents,
  )} cash`;
  return (
    <g className={`city-tile${dead ? ' city-dead' : ''}`}>
      <title>{title}</title>
      <rect x={x} y={y} width={TILE_W} height={TILE_H} rx={7} />
      <text className="city-name" x={x + 8} y={y + 18}>
        {b.name.length > 14 ? `${b.name.slice(0, 13)}…` : b.name}
      </text>
      <text className="city-sub" x={x + 8} y={y + 34}>
        {b.kind} · {b.workers}/{b.target_workers}
      </text>
    </g>
  );
}

function CivicTile({
  x,
  y,
  name,
  sub,
  title,
}: {
  x: number;
  y: number;
  name: string;
  sub: string;
  title: string;
}) {
  return (
    <g className="city-tile city-civic">
      <title>{title}</title>
      <rect x={x} y={y} width={TILE_W} height={TILE_H} rx={7} />
      <text className="city-name" x={x + 8} y={y + 18}>
        {name}
      </text>
      <text className="city-sub" x={x + 8} y={y + 34}>
        {sub}
      </text>
    </g>
  );
}

const ZONES: { label: string; kinds: string[]; x: number; perRow: number }[] = [
  { label: 'Farmland', kinds: ['farm'], x: 12, perRow: 2 },
  { label: 'Town', kinds: ['mill', 'bakery'], x: 240, perRow: 2 },
  {
    label: 'Industry',
    kinds: ['mine', 'steel mill', 'tool factory'],
    x: 468,
    perRow: 2,
  },
  {
    label: 'Works',
    kinds: ['lumber camp', 'brickworks', 'construction co'],
    x: 696,
    perRow: 2,
  },
];

export function CityView({
  businesses,
  agents,
  stats,
  onSelectAgent,
}: {
  businesses: BusinessRow[];
  agents: AgentRow[];
  stats: Stats;
  onSelectAgent: (id: number) => void;
}) {
  const zoneTop = 26;
  let zonesBottom = zoneTop;
  const tiles: Placed[] = [];
  const labels: { x: number; text: string }[] = [];
  for (const zone of ZONES) {
    const members = businesses.filter((b) => zone.kinds.includes(b.kind));
    const { placed, bottom } = layout(members, zone.x, zoneTop, zone.perRow);
    tiles.push(...placed);
    labels.push({ x: zone.x, text: zone.label });
    zonesBottom = Math.max(zonesBottom, bottom);
  }

  // Civic column on the far right: the bank and the government.
  const civicX = 924;
  const civic = [
    {
      name: 'Town Bank',
      sub: `rate ${formatBp(stats.bank_rate_bp)}`,
      title: `Town Bank\nbase rate ${formatBp(stats.bank_rate_bp)}/yr`,
    },
    {
      name: 'Government',
      sub: formatMoney(stats.govt_cash_cents),
      title: `Government\ntreasury ${formatMoney(stats.govt_cash_cents)} · debt ${formatMoney(
        stats.govt_debt_cents,
      )}`,
    },
  ];
  labels.push({ x: civicX, text: 'Civic' });

  // Residential strip below the zones: one house per resident.
  const houseTop = Math.max(zonesBottom, zoneTop + 2 * (TILE_H + GAP)) + 22;
  const perRow = 44;
  const houseW = 23;
  const houseRows = Math.ceil(agents.length / perRow);
  const height = houseTop + houseRows * 26 + 12;

  return (
    <svg
      className="city"
      viewBox={`0 0 1030 ${height}`}
      role="img"
      aria-label="City map"
    >
      {labels.map((l) => (
        <text className="city-zone" key={l.text} x={l.x} y={16}>
          {l.text.toUpperCase()}
        </text>
      ))}
      {tiles.map((p) => (
        <BusinessTile key={p.b.id} {...p} />
      ))}
      {civic.map((c, i) => (
        <CivicTile
          key={c.name}
          x={civicX}
          y={zoneTop + i * (TILE_H + GAP)}
          {...c}
        />
      ))}
      <text className="city-zone" x={12} y={houseTop - 6}>
        RESIDENTIAL
      </text>
      {agents.map((a, i) => {
        const x = 12 + (i % perRow) * houseW;
        const y = houseTop + Math.floor(i / perRow) * 26;
        const cls = `city-house${a.owns_home ? ' city-owned' : ''}${
          a.hungry_streak > 0 ? ' city-hungry' : ''
        }`;
        const status = [
          a.role,
          a.owns_home ? 'homeowner' : 'renter',
          a.hungry_streak > 0 ? `hungry ${a.hungry_streak}d` : 'fed',
        ].join(' · ');
        return (
          <g
            className={cls}
            key={a.id}
            onClick={() => onSelectAgent(a.id)}
            role="button"
            aria-label={a.name}
          >
            <title>{`${a.name}\n${status}`}</title>
            <polygon
              points={`${x},${y + 8} ${x + 8},${y} ${x + 16},${y + 8}`}
            />
            <rect x={x + 1.5} y={y + 8} width={13} height={9} />
          </g>
        );
      })}
    </svg>
  );
}
