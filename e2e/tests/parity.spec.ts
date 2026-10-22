import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { expect, test } from '@playwright/test';

import { APPS, Pair } from './apps';

/** The largest share of pixels allowed to differ between the two apps. */
const MAX_DIFF = 0.005;
const VIEWPORTS = [
  { name: 'phone', width: 390, height: 844 },
  { name: 'desktop', width: 1280, height: 800 },
];
const STATES: Record<string, (pair: Pair) => Promise<void>> = {
  empty: async () => {},
  errors: async (pair) => {
    await pair.click('button');
    await pair.blur();
  },
  success: async (pair) => {
    await pair.fillValid();
    await pair.signUp(() => pair.press('Enter'));
    await pair.react.getByRole('heading', { name: 'Account created' }).waitFor();
    await pair.blur();
  },
};
// Set by `pnpm screenshots` to save the images shown in the README.
const SCREENSHOTS = process.env.SCREENSHOTS;

for (const viewport of VIEWPORTS) {
  for (const [state, reach] of Object.entries(STATES)) {
    test(`${state} looks the same on ${viewport.name}`, async ({ browser }) => {
      const pair = await Pair.open(browser, viewport);
      await reach(pair);
      const { shots, differing, total } = await pair.compare(true);
      await pair.close();
      if (SCREENSHOTS) {
        mkdirSync(SCREENSHOTS, { recursive: true });
        APPS.forEach((app, index) => {
          writeFileSync(join(SCREENSHOTS, `${app.name}-${state}-${viewport.name}.png`), shots[index]);
        });
      }
      const ratio = differing / total;
      test.info().annotations.push({ type: 'differing pixels', description: `${differing} (${(ratio * 100).toFixed(3)}%)` });
      expect(ratio).toBeLessThanOrEqual(MAX_DIFF);
    });
  }
}
