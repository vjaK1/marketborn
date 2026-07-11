# Marketborn

A deterministic economic-agent strategy simulation for the desktop: a fictional city-state of
autonomous agents who work, trade, produce, save and go hungry — observed and influenced (never
commanded) by the player. Tauri 2 + React/TypeScript UI over a pure-Rust simulation core with
SQLite persistence.

## Quickstart

Prerequisites: Rust (pinned by `rust-toolchain.toml`), Node 20+, VS Build Tools (Windows),
WebView2 runtime.

```
npm --prefix app install     # first time only
npm run check                # full quality gate: fmt, clippy, tests, tsc, vitest
npm run app:desktop          # build the frontend and launch the desktop app
cargo run -p sim-cli -- run --seed 42 --ticks 365   # headless simulation
```

## Layout

- `crates/sim-core` — pure simulation logic (no I/O, no clock, no threads)
- `crates/sim-persist` — SQLite save/load/replay/event archive
- `crates/sim-cli` — headless runner: `run`, `replay`, `hash`, `diff`
- `src-tauri` — thin desktop shell hosting the simulation thread
- `app` — React + Vite + TypeScript frontend
- `docs` — the project's binding documentation (start with `BRIEF.md`, `PLAN.md`, `PROGRESS.md`)

`CLAUDE.md` is the project constitution — binding for every working session.
