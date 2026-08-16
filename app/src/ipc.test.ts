/**
 * The websocket transport's client half: connection handshake, snapshot
 * dispatch, request/reply correlation, and failure paths — driven against
 * a scripted fake WebSocket.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WorldSnapshot } from './types';
import { getAgentDetail, initIpc, sendSpeed } from './ipc';

class FakeWebSocket {
  static instances: FakeWebSocket[] = [];

  url: string;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  // Test choreography:
  open(): void {
    this.onopen?.();
  }
  receive(body: unknown): void {
    this.onmessage?.({ data: JSON.stringify(body) });
  }
  close(): void {
    this.onclose?.();
  }
  lastSent(): Record<string, unknown> {
    const raw = this.sent.at(-1);
    if (!raw) throw new Error('nothing sent');
    return JSON.parse(raw) as Record<string, unknown>;
  }
}

async function connected(
  onSnapshot: (s: WorldSnapshot) => void = () => {},
): Promise<FakeWebSocket> {
  const init = initIpc(onSnapshot);
  const ws = FakeWebSocket.instances.at(-1);
  if (!ws) throw new Error('no socket constructed');
  ws.open();
  expect(await init).toBe(true);
  return ws;
}

describe('the websocket transport', () => {
  vi.stubGlobal('WebSocket', FakeWebSocket);

  afterEach(() => {
    // Leave no half-open module state between tests.
    FakeWebSocket.instances.at(-1)?.close();
    FakeWebSocket.instances.length = 0;
  });

  it('connects, and dispatches snapshot pushes', async () => {
    const seen: WorldSnapshot[] = [];
    const ws = await connected((s) => seen.push(s));
    ws.receive({ kind: 'snapshot', data: { tick: 7 } });
    expect(seen).toHaveLength(1);
    expect(seen[0]?.tick).toBe(7);
  });

  it('reports a connection that never opens', async () => {
    const init = initIpc(() => {});
    FakeWebSocket.instances.at(-1)?.close();
    expect(await init).toBe(false);
  });

  it('correlates replies to requests by id', async () => {
    const ws = await connected();
    const speedDone = sendSpeed(4);
    const speedReq = ws.lastSent();
    expect(speedReq.kind).toBe('set_speed');
    expect(speedReq.level).toBe(4);
    const detailWanted = getAgentDetail(3);
    const detailReq = ws.lastSent();
    expect(detailReq.kind).toBe('agent_detail');
    expect(detailReq.id).toBe(3);
    // Replies arrive out of order; each lands on its own request.
    ws.receive({
      kind: 'reply',
      req: detailReq.req,
      ok: true,
      data: { name: 'Agent Three' },
    });
    ws.receive({ kind: 'reply', req: speedReq.req, ok: true, data: null });
    await speedDone;
    expect(await detailWanted).toEqual({ name: 'Agent Three' });
  });

  it('rejects a failed reply with the server error', async () => {
    const ws = await connected();
    const wanted = getAgentDetail(9);
    const req = ws.lastSent();
    ws.receive({ kind: 'reply', req: req.req, ok: false, error: 'no such agent' });
    await expect(wanted).rejects.toThrow('no such agent');
  });

  it('fails every pending request when the backend disconnects', async () => {
    const ws = await connected();
    const wanted = getAgentDetail(1);
    ws.close();
    await expect(wanted).rejects.toThrow('backend disconnected');
  });
});
