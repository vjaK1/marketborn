import { create } from 'zustand';
import type { WorldSnapshot } from './types';

export interface UiState {
  /** True once the Tauri backend has produced at least one snapshot. */
  connected: boolean;
  snapshot: WorldSnapshot | null;
  /** 0 = paused · 1..3 = paced · 4 = max. Mirrors the shell's sim thread. */
  speed: number;
  saveMessage: string | null;
  applySnapshot: (s: WorldSnapshot) => void;
  setSpeed: (level: number) => void;
  setSaveMessage: (message: string | null) => void;
}

export const SPEED_LEVELS = [
  { level: 0, label: '⏸', title: 'Pause' },
  { level: 1, label: '▶', title: 'Run (2 days/s)' },
  { level: 2, label: '▶▶', title: 'Fast (10 days/s)' },
  { level: 3, label: '▶▶▶', title: 'Very fast (50 days/s)' },
  { level: 4, label: 'Max', title: 'As fast as possible' },
] as const;

export const useStore = create<UiState>((set) => ({
  connected: false,
  snapshot: null,
  speed: 1,
  saveMessage: null,
  applySnapshot: (s) => set({ snapshot: s, connected: true }),
  setSpeed: (level) => set({ speed: Math.max(0, Math.min(4, Math.trunc(level))) }),
  setSaveMessage: (message) => set({ saveMessage: message }),
}));
