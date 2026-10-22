import type { Browser, Page } from '@playwright/test';
import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';

export const APPS = [
  { name: 'react', url: process.env.REACT_URL ?? 'http://localhost:8081/' },
  { name: 'wasm', url: process.env.WASM_URL ?? 'http://localhost:8082/' },
] as const;

export const ADA = {
  fullName: 'Ada Lovelace',
  email: 'ada@example.com',
  password: 'Analytical#Engine1',
};

export const SIGN_UP = '**/signup.v1.SignupService/SignUp';

/** Opens an app and waits until its form is ready and painted. */
export async function open(page: Page, url: string): Promise<void> {
  await page.goto(url);
  // The canvas app says it is busy until it has painted.
  await page.locator('#fullName, canvas[aria-busy="false"]').first().waitFor({ state: 'attached' });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise(requestAnimationFrame);
  });
}

/**
 * Both apps open side by side and driven alike, with the keyboard and the
 * mouse only: the canvas app has no elements to find, so clicks go where
 * the React app's elements are, as both lay the page out the same way.
 */
export class Pair {
  private constructor(
    readonly react: Page,
    readonly wasm: Page,
  ) {}

  static async open(browser: Browser, viewport: { width: number; height: number }): Promise<Pair> {
    const [react, wasm] = await Promise.all(
      APPS.map(async (app) => {
        const page = await browser.newPage({ viewport });
        await open(page, app.url);
        return page;
      }),
    );
    return new Pair(react, wasm);
  }

  get pages(): Page[] {
    return [this.react, this.wasm];
  }

  async each(step: (page: Page) => Promise<unknown>): Promise<void> {
    for (const page of this.pages) await step(page);
  }

  async press(...keys: string[]): Promise<void> {
    await this.each(async (page) => {
      for (const key of keys) await page.keyboard.press(key);
    });
  }

  async type(text: string): Promise<void> {
    await this.each((page) => page.keyboard.type(text));
  }

  /**
   * Clicks both apps on the React app's `selector`: at its middle, or `x`
   * pixels from its left edge.
   */
  async click(selector: string, options: { x?: number; clickCount?: number } = {}): Promise<void> {
    const box = await this.react.locator(selector).boundingBox();
    if (!box) throw new Error(`${selector} is not on screen`);
    const x = box.x + (options.x ?? box.width / 2);
    await this.clickAt(x, box.y + box.height / 2, options.clickCount);
  }

  async clickAt(x: number, y: number, clickCount = 1): Promise<void> {
    await this.each((page) => page.mouse.click(x, y, { clickCount }));
  }

  /** Moves focus off the controls, onto the page, so no caret or focus ring shows. */
  async blur(): Promise<void> {
    await this.clickAt(2, 2);
  }

  /** Fills every field with valid values, leaving focus on the checkbox. */
  async fillValid(): Promise<void> {
    for (const value of [ADA.fullName, ADA.email, ADA.password, ADA.password]) {
      await this.press('Tab');
      await this.type(value);
    }
    await this.press('Tab', 'Space');
  }

  /** Waits for both apps' sign-up calls, made by `submit`. */
  async signUp(submit: () => Promise<void>) {
    const calls = Promise.all(this.pages.map((page) => page.waitForResponse(SIGN_UP)));
    await submit();
    const responses = await calls;
    await this.settled();
    return responses;
  }

  /** Waits until the canvas app has handled the backend's answer and both have painted. */
  async settled(): Promise<void> {
    await this.wasm.locator('canvas[aria-busy="false"]').waitFor({ state: 'attached' });
    await this.each((page) => page.evaluate(() => new Promise(requestAnimationFrame)));
  }

  /** Both apps' screenshots and how many of their pixels differ. */
  async compare(fullPage = false): Promise<{ shots: Buffer[]; differing: number; total: number }> {
    await this.settled();
    const shots = await Promise.all(this.pages.map((page) => page.screenshot({ fullPage })));
    const [react, wasm] = shots.map((shot) => PNG.sync.read(shot));
    if (react.width !== wasm.width || react.height !== wasm.height) {
      return { shots, differing: Infinity, total: react.width * react.height };
    }
    const differing = pixelmatch(react.data, wasm.data, null, react.width, react.height, { threshold: 0.1 });
    return { shots, differing, total: react.width * react.height };
  }

  async close(): Promise<void> {
    await Promise.all(this.pages.map((page) => page.close()));
  }
}
