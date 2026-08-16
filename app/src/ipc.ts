/**
 * The UI's only doorway to the simulation. Inbound: `snapshot` events pushed
 * by the backend at ≤ 10 Hz plus a pull on startup. Outbound: speed changes,
 * save requests and detail queries. The UI never mutates simulation state
 * directly.
 *
 * Two transports implement the same protocol (docs/ARCHITECTURE.md):
 * - The Tauri desktop shell (dynamic imports so the bundle also loads in a
 *   plain browser).
 * - `sim-cli serve` over websocket, for the browser dev preview and the
 *   Playwright E2E suite. The URL defaults to ws://127.0.0.1:17771 and can
 *   be overridden with a `?ws=` query parameter.
 */

import type { AgentDetail, ContractDetail, WorldSnapshot } from './types';

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

// --- Websocket transport state ---

interface ServerReply {
  kind: 'reply';
  req?: number;
  ok: boolean;
  data?: unknown;
  error?: string;
}

interface ServerSnapshot {
  kind: 'snapshot';
  data: WorldSnapshot;
}

type ServerMsg = ServerReply | ServerSnapshot;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

let socket: WebSocket | null = null;
let nextReq = 1;
const pending = new Map<number, Pending>();

export function wsUrl(): string {
  if (typeof window !== 'undefined') {
    const q = new URLSearchParams(window.location.search).get('ws');
    if (q) return q;
  }
  return 'ws://127.0.0.1:17771';
}

function failAllPending(reason: string): void {
  for (const p of pending.values()) p.reject(new Error(reason));
  pending.clear();
}

function connectWs(onSnapshot: (s: WorldSnapshot) => void): Promise<boolean> {
  return new Promise((resolve) => {
    let ws: WebSocket;
    try {
      ws = new WebSocket(wsUrl());
    } catch {
      resolve(false);
      return;
    }
    let opened = false;
    ws.onopen = () => {
      opened = true;
      socket = ws;
      resolve(true);
    };
    ws.onerror = () => {
      if (!opened) resolve(false);
    };
    ws.onclose = () => {
      if (socket === ws) socket = null;
      failAllPending('backend disconnected');
      if (!opened) resolve(false);
    };
    ws.onmessage = (event: MessageEvent) => {
      let msg: ServerMsg;
      try {
        msg = JSON.parse(event.data as string) as ServerMsg;
      } catch {
        return;
      }
      if (msg.kind === 'snapshot') {
        onSnapshot(msg.data);
      } else if (msg.kind === 'reply' && typeof msg.req === 'number') {
        const p = pending.get(msg.req);
        if (p) {
          pending.delete(msg.req);
          if (msg.ok) p.resolve(msg.data ?? null);
          else p.reject(new Error(msg.error ?? 'request failed'));
        }
      }
    };
  });
}

function wsRequest<T>(payload: Record<string, unknown>): Promise<T> {
  if (!socket) return Promise.reject(new Error('backend not connected'));
  const req = nextReq;
  nextReq += 1;
  return new Promise<T>((resolve, reject) => {
    pending.set(req, {
      resolve: (value) => resolve(value as T),
      reject,
    });
    socket?.send(JSON.stringify({ ...payload, req }));
  });
}

// --- The transport-agnostic surface the app uses ---

export async function initIpc(
  onSnapshot: (s: WorldSnapshot) => void,
): Promise<boolean> {
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    await listen<WorldSnapshot>('snapshot', (event) =>
      onSnapshot(event.payload),
    );
    const { invoke } = await import('@tauri-apps/api/core');
    const initial = await invoke<WorldSnapshot | null>('get_snapshot');
    if (initial) onSnapshot(initial);
    return true;
  }
  return connectWs(onSnapshot);
}

export async function sendSpeed(level: number): Promise<void> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_speed', { level });
    return;
  }
  if (socket) await wsRequest<null>({ kind: 'set_speed', level });
}

export async function saveGame(): Promise<string> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string>('save_game');
  }
  if (socket) return wsRequest<string>({ kind: 'save' });
  throw new Error('no backend connected');
}

export async function getAgentDetail(id: number): Promise<AgentDetail | null> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<AgentDetail | null>('get_agent_detail', { id });
  }
  if (socket) return wsRequest<AgentDetail | null>({ kind: 'agent_detail', id });
  return null;
}

export async function getContractDetail(
  id: number,
): Promise<ContractDetail | null> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<ContractDetail | null>('get_contract_detail', { id });
  }
  if (socket) {
    return wsRequest<ContractDetail | null>({ kind: 'contract_detail', id });
  }
  return null;
}

/**
 * Queue a player command for the next tick boundary (the levers). The
 * command shape is serde's external tagging of `PlayerCommand`, e.g.
 * `{ SetSalesTax: { rate_bp: 500 } }`.
 */
export async function queueCommand(
  command: Record<string, unknown>,
): Promise<{ seq: number; tick: number }> {
  if (isTauri()) {
    // The desktop shell gains its command channel with the policy screen
    // (Phase 5); until then commands are a serve-transport feature.
    throw new Error('commands are not yet wired in the desktop shell');
  }
  if (socket) {
    return wsRequest<{ seq: number; tick: number }>({
      kind: 'queue_command',
      command,
    });
  }
  throw new Error('no backend connected');
}
