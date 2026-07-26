/**
 * The UI's only doorway to the simulation. Inbound: `snapshot` events pushed
 * by the shell at ≤ 10 Hz plus a pull on startup. Outbound: speed changes and
 * save requests. The UI never mutates simulation state directly.
 *
 * Tauri modules are imported dynamically so the bundle also loads in a plain
 * browser (dev preview / tests) where it shows a "waiting for backend" state.
 */

import type { AgentDetail, WorldSnapshot } from './types';

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function initIpc(
  onSnapshot: (s: WorldSnapshot) => void,
): Promise<boolean> {
  if (!isTauri()) return false;
  const { listen } = await import('@tauri-apps/api/event');
  await listen<WorldSnapshot>('snapshot', (event) => onSnapshot(event.payload));
  const { invoke } = await import('@tauri-apps/api/core');
  const initial = await invoke<WorldSnapshot | null>('get_snapshot');
  if (initial) onSnapshot(initial);
  return true;
}

export async function sendSpeed(level: number): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('set_speed', { level });
}

export async function saveGame(): Promise<string> {
  if (!isTauri()) throw new Error('not running inside the desktop shell');
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('save_game');
}

export async function getAgentDetail(id: number): Promise<AgentDetail | null> {
  if (!isTauri()) return null;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AgentDetail | null>('get_agent_detail', { id });
}
