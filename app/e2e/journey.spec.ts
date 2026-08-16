import { expect, test, type Page } from '@playwright/test';

/**
 * The BRIEF's E2E list as one player journey through a single living
 * world: a new world appears, speed changes, an agent and a business get
 * inspected, a policy is applied and takes effect, the world is saved,
 * run forward, and loaded back — the rewind. Plus the chart tab and the
 * city map's house click.
 *
 * One serial test: the backend is one shared world, and the journey
 * reads better than isolated fragments.
 */

const panel = (page: Page, heading: string | RegExp) =>
  page.locator('section.panel', {
    has: page.getByRole('heading', { name: heading }),
  });

const pause = async (page: Page) => {
  await page.getByTitle('Pause').click();
  // Let queued pushes drain so the shown date is the resting date.
  await page.waitForTimeout(600);
};

test('a full player journey over the websocket transport', async ({
  page,
}) => {
  await page.goto('/');

  await test.step('a new world is visible and alive', async () => {
    await expect(page.locator('.brand').first()).toContainText('Marketborn');
    await expect(
      page.locator('.chip', { hasText: 'Population' }).locator('.value'),
    ).toHaveText('29');
    const date = page.locator('.date');
    const before = (await date.textContent()) ?? '';
    await expect(date).not.toHaveText(before, { timeout: 10_000 });
  });

  await test.step('speed controls race and pause the world', async () => {
    await page.getByTitle('As fast as possible').click();
    const date = page.locator('.date');
    const t0 = (await date.textContent()) ?? '';
    await expect(date).not.toHaveText(t0, { timeout: 5_000 });
    await pause(page);
    const frozen = (await date.textContent()) ?? '';
    await page.waitForTimeout(1200);
    await expect(date).toHaveText(frozen);
  });

  await test.step('the agent inspector opens from the table', async () => {
    const agents = panel(page, 'Agents');
    await agents.locator('tbody tr').first().click();
    await expect(
      page.getByRole('heading', { name: 'Agent inspector' }),
    ).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'Personality' }),
    ).toBeVisible();
    await page.getByRole('button', { name: '← All agents' }).click();
    await expect(
      page.getByRole('heading', { name: 'Agents', exact: true }),
    ).toBeVisible();
  });

  await test.step('the business inspector opens from the table', async () => {
    const businesses = panel(page, 'Businesses');
    await businesses.locator('tbody tr').first().click();
    await expect(
      page.getByRole('heading', { name: 'Business inspector' }),
    ).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'Balance sheet' }),
    ).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'Lifetime books' }),
    ).toBeVisible();
    await page.getByRole('button', { name: '← All businesses' }).click();
  });

  await test.step('a policy is enacted and takes effect', async () => {
    const row = page.locator('.policy-table tr', { hasText: 'Sales tax' });
    await expect(row.locator('.policy-current')).toHaveText('1%');
    await row.locator('input').fill('5');
    await row.getByRole('button', { name: 'Enact' }).click();
    await expect(row.locator('.policy-note')).toContainText(
      'takes effect day',
    );
    // Commands apply at the next tick boundary; the world is paused, so
    // run it forward one beat and watch the readback change.
    await page.getByTitle('Run (2 days/s)').click();
    await expect(row.locator('.policy-current')).toHaveText('5%', {
      timeout: 10_000,
    });
    await pause(page);
    // The event log carries the policy event.
    await panel(page, 'Event log')
      .getByRole('button', { name: 'Government' })
      .click();
    await expect(page.locator('.event-rows')).toContainText(
      'sales tax moved',
      { timeout: 5_000 },
    );
    await panel(page, 'Event log')
      .getByRole('button', { name: 'All', exact: true })
      .click();
  });

  await test.step('save, run forward, load — the rewind', async () => {
    const date = page.locator('.date');
    const savedDate = (await date.textContent()) ?? '';
    await page.getByRole('button', { name: /Saves/ }).click();
    const slotRow = page.locator('.save-row', { hasText: 'slot-1' });
    await slotRow.getByRole('button', { name: 'Save' }).click();
    await expect(page.locator('.toast')).toContainText('Saved slot-1');
    // The menu is a toggle; close it so the next open is an open.
    await page.getByRole('button', { name: /Saves/ }).click();
    await expect(page.locator('.save-row').first()).toBeHidden();

    await page.getByTitle('As fast as possible').click();
    await expect(date).not.toHaveText(savedDate, { timeout: 5_000 });
    await pause(page);

    await page.getByRole('button', { name: /Saves/ }).click();
    await page
      .locator('.save-row', { hasText: 'slot-1' })
      .getByRole('button', { name: 'Load' })
      .click();
    await expect(page.locator('.toast')).toContainText('Loaded slot-1');
    await expect(date).toHaveText(savedDate, { timeout: 5_000 });
  });

  await test.step('the society chart tab renders', async () => {
    await page.getByRole('button', { name: 'Society & treasury' }).click();
    await expect(page.locator('.chart-host canvas').first()).toBeVisible();
  });

  await test.step('a city house opens its resident', async () => {
    await page.locator('g.city-house').first().click();
    await expect(
      page.getByRole('heading', { name: 'Agent inspector' }),
    ).toBeVisible();
  });
});
