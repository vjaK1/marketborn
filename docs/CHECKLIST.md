# Per-screen manual checklist (Phase 5 done-when)

The written walk of every v1.0 screen, verified against the running app.
Two surfaces carry the product: the browser against `sim-cli serve` (the
E2E transport) and the packaged desktop exe. Each item names what to
look at and records the verification evidence (session numbers refer to
PROGRESS.md; the E2E suite re-verifies the interactive items on every
`check:full`).

Status legend: ✅ verified · ⚠ verified with a recorded caveat.

## World overview (header + stat chips)

- [x] ✅ Brand, date (Y·D), and halted banner slot render; the date
  advances at the selected speed. *(E2E step 1; packaged smoke: Y1·D14 →
  Y6·D301.)*
- [x] ✅ Speed controls: pause freezes the date, Max races it; the
  active level is highlighted. *(E2E step 2.)*
- [x] ✅ Stat chips: population, employed/unemployed, hungry (red when
  nonzero), money supply, GDP (7d), food inflation (90d — em-dash until
  day 97), cash Gini, food price, food on shelves, bank rate, treasury,
  government debt (amber when nonzero). *(Sessions 11–16 captures; the
  packaged smoke shows the full row live.)*

## City view

- [x] ✅ Zones (farmland/town/industry/works), one tile per business
  with staffing; dead businesses get red dashed borders — verified
  organically (lumber camp + brickworks dead at day 180, session 12).
- [x] ✅ Civic column: bank (rate) and government (treasury) live.
- [x] ✅ Residential strip: filled = homeowner, hollow = renter, red =
  hungry; count matches the hungry chip. Clicking a house opens its
  resident's inspector. *(Session 12 + E2E step 8.)*
- [x] ✅ Business tiles open the business inspector. *(Session 13.)*
- [x] ✅ Panel collapses to yield space.

## Agent inspector

- [x] ✅ Identity, role, workplace, cash/pantry/home/hunger, earned and
  spent; nine personality traits; recent decisions with the engine's
  explanations verbatim; memories; relationships (seven dimensions);
  beliefs. *(Phase 2 acceptance + E2E step 3.)*

## Business inspector

- [x] ✅ Identity and staffing, pricing and expectations, weekly and
  lifetime profit; balance sheet at market valuation (equity identity
  unit-tested); credit standing incl. prior defaults; contracts on both
  sides; lifetime books as signed flows; staff roster; recent history.
  *(Session 13 + E2E step 4.)*

## Market view

- [x] ✅ Per-good rows: last price, volume, unmet demand (red), rot,
  best ask, offered, demand (red when unmet), world stock. *(Phase 1
  screen; visible in every session capture.)*

## Contract view

- [x] ✅ Table: parties, good, ceiling, price, delivered/missed
  tallies, state chips (active/completed/breached/terminated).
  *(Phase 3 launch verification.)*
- [x] ✅ Inspector: terms, tallies, penalties, the negotiation move by
  move, event history (honestly ring-bounded). *(Phase 3, session 6.)*

## Event timeline

- [x] ✅ Newest-first log with kind dots and readable text; filter
  chips (People/Business/Contracts/Finance/Government) and free-text
  filter; honest empty state. *(Session 12 + E2E step 5 asserts the
  Government filter carries the policy event.)*

## Government / policy panel

- [x] ✅ Budget line (treasury + sovereign debt) and five levers (sales
  tax, bank rate, welfare floor, minimum wage, deficit limit): current
  value readback, unit-labelled input, Enact → "takes effect day N",
  readback changes after the tick boundary. *(Session 11 + E2E step 5.)*
- [x] ⚠ Lever pull verified over the serve transport (browser + E2E).
  The desktop shell's `queue_command` glue is compile-verified with an
  identical pattern to its four runtime-proven handlers, but has not
  been clicked interactively — the machine was in active use whenever
  the desktop app was up (sessions 11, 16). Re-check opportunistically.

## Charts

- [x] ✅ Prices — daily average: all flow goods, direct end labels,
  tooltip; window scrolls with history. *(Every session; packaged
  smoke.)*
- [x] ✅ Society & treasury tab: employment + hunger (counts) and
  treasury + debt ($) over the same window. *(E2E step 7 renders it;
  session 14 mount capture.)*

## Saves

- [x] ✅ Saves menu: three player slots with saved dates, load-only
  rows for autosave/quicksave; save writes, load rewinds the date.
  *(E2E step 6 — the rewind is asserted; serve protocol test covers
  slots/listing/hostile names.)*
- [x] ✅ Autosave: 60 s wall clock, packaged app writes
  `%APPDATA%\com.marketborn.app\saves\autosave.mbsave` hands-free.
  *(Session 16 packaged smoke.)*

## Packaged app (smoke)

- [x] ✅ NSIS installer builds (`npm run app:package` →
  `target/release/bundle/nsis/Marketborn_0.1.0_x64-setup.exe`).
- [x] ✅ The release exe launches, creates a world, ticks advance
  (Y1·D14 → Y6·D301 observed), all screens render, autosave persists.
  *(Session 16; captures in the session record.)*
