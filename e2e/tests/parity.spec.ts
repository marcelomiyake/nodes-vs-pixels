import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { expect, test, type Page } from '@playwright/test';
import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';

import { APPS, blur, fillValid, open, submit } from './apps';

/** The largest share of pixels allowed to differ between the two apps. */
const MAX_DIFF = 0.005;
const VIEWPORTS = [
  { name: 'phone', width: 390, height: 844 },
  { name: 'desktop', width: 1280, height: 800 },
];
const STATES: Record<string, (page: Page) => Promise<void>> = {
  empty: async () => {},
  errors: async (page) => {
    await submit(page);
    await blur(page);
  },
  success: async (page) => {
    await fillValid(page);
    await submit(page);
    await page.getByRole('heading', { name: 'Account created' }).waitFor();
    await blur(page);
  },
};
// Set by `pnpm screenshots` to save the images shown in the README.
const SCREENSHOTS = process.env.SCREENSHOTS;

for (const viewport of VIEWPORTS) {
  for (const [state, reach] of Object.entries(STATES)) {
    test(`${state} looks the same on ${viewport.name}`, async ({ browser }) => {
      const shots: PNG[] = [];
      for (const app of APPS) {
        const page = await browser.newPage({ viewport });
        await open(page, app.url);
        await reach(page);
        const image = await page.screenshot({ fullPage: true });
        if (SCREENSHOTS) {
          mkdirSync(SCREENSHOTS, { recursive: true });
          writeFileSync(join(SCREENSHOTS, `${app.name}-${state}-${viewport.name}.png`), image);
        }
        shots.push(PNG.sync.read(image));
        await page.close();
      }
      const [react, wasm] = shots;
      expect([wasm.width, wasm.height]).toEqual([react.width, react.height]);
      const differing = pixelmatch(react.data, wasm.data, null, react.width, react.height, { threshold: 0.1 });
      const ratio = differing / (react.width * react.height);
      test.info().annotations.push({ type: 'differing pixels', description: `${differing} (${(ratio * 100).toFixed(3)}%)` });
      expect(ratio).toBeLessThanOrEqual(MAX_DIFF);
    });
  }
}
