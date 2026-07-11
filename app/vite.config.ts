/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  // Tauri serves the built assets from a fixed origin; relative base keeps
  // asset URLs working there.
  base: './',
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: 'es2022',
    sourcemap: false,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
