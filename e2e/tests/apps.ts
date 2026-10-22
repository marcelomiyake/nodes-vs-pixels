import type { Page } from '@playwright/test';

export const APPS = [
  { name: 'react', url: process.env.REACT_URL ?? 'http://localhost:8081/' },
  { name: 'wasm', url: process.env.WASM_URL ?? 'http://localhost:8082/' },
] as const;

export const ADA = {
  fullName: 'Ada Lovelace',
  email: 'ada@example.com',
  password: 'Analytical#Engine1',
};

/** Opens an app and waits until its form is ready and painted. */
export async function open(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await page.locator('#fullName').waitFor({ state: 'attached' });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise(requestAnimationFrame);
  });
}

export async function fillValid(page: Page): Promise<void> {
  await page.locator('#fullName').fill(ADA.fullName);
  await page.locator('#email').fill(ADA.email);
  await page.locator('#password').fill(ADA.password);
  await page.locator('#confirmPassword').fill(ADA.password);
  await page.locator('#acceptTerms').check();
}

export async function submit(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Create account' }).click();
}

/** Moves focus away so no caret or focus ring is captured. */
export async function blur(page: Page): Promise<void> {
  await page.evaluate(() => {
    (document.activeElement as HTMLElement | null)?.blur();
  });
}
