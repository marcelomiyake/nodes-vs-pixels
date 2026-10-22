import { expect, test } from '@playwright/test';

import { ADA, APPS, Pair, SIGN_UP, open } from './apps';

const VIEWPORT = { width: 1280, height: 1000 };
/** Pixels that may differ between the apps in one state: a caret in another phase of its blink. */
const SAME = 100;

test.describe('both apps, driven alike', () => {
  let pair: Pair;
  test.beforeEach(async ({ browser }) => {
    pair = await Pair.open(browser, VIEWPORT);
  });
  test.afterEach(async () => {
    await pair.close();
  });

  const expectSame = async () => {
    const { differing } = await pair.compare();
    expect(differing).toBeLessThanOrEqual(SAME);
  };

  test('flag every field and focus the first on an empty submit', async () => {
    await pair.click('button');
    await expect(pair.react.locator('#fullName')).toBeFocused();
    await expect(pair.react.locator('#fullName-message')).toHaveText('Enter your full name.');
    await expect(pair.react.locator('#acceptTerms-message')).toHaveText('Accept the terms to continue.');
    await expectSame();
  });

  test('check a field when it loses focus', async () => {
    await pair.press('Tab', 'Tab', 'Tab');
    await pair.type('short');
    await pair.press('Tab');
    await expect(pair.react.locator('#password-message')).toHaveText('Password must be at least 12 characters.');
    await expectSame();
    // Tabbing back selects the value, which typing replaces.
    await pair.press('Shift+Tab');
    await pair.type(ADA.password);
    await expect(pair.react.locator('#password-message')).toHaveText(
      'Use upper and lowercase letters, a number and a symbol.',
    );
    await expectSame();
  });

  test('edit text with the keyboard and the mouse', async () => {
    await pair.click('#fullName');
    await pair.type('Ada Byron');
    await pair.press('Control+Backspace');
    await pair.type('Lovelace');
    await pair.press('Home', 'Control+ArrowRight', 'Shift+End');
    await expect(pair.react.locator('#fullName')).toHaveValue('Ada Lovelace');
    await expectSame();
    await pair.click('#email');
    await pair.type(ADA.email);
    // A double click selects the word under it.
    await pair.click('#email', { clickCount: 2, x: 40 });
    await expectSame();
    await pair.click('#acceptTerms');
    await expect(pair.react.locator('#acceptTerms')).toBeChecked();
    await expectSame();
  });

  test('sign up through the backend over gRPC-web', async () => {
    await pair.fillValid();
    const calls = await pair.signUp(() => pair.press('Enter'));
    for (const call of calls) {
      expect(call.status()).toBe(200);
      expect(call.request().headers()['content-type']).toBe('application/grpc-web+proto');
      expect(await call.headerValue('content-type')).toContain('application/grpc-web');
    }
    await expect(pair.react.getByRole('heading', { name: 'Account created' })).toBeFocused();
    await expect(pair.react.getByText(`Welcome aboard, ${ADA.fullName}. Your account is ready.`)).toBeVisible();
    await expectSame();
  });

  test('report an unreachable server and allow a retry', async () => {
    await pair.each((page) => page.route(SIGN_UP, (route) => route.abort()));
    await pair.fillValid();
    await pair.press('Enter');
    await expect(pair.react.getByRole('alert')).toHaveText("We couldn't reach the server. Please try again.");
    await expectSame();
    await pair.each((page) => page.unroute(SIGN_UP));
    await pair.signUp(() => pair.click('button'));
    await expect(pair.react.getByRole('heading', { name: 'Account created' })).toBeVisible();
    await expectSame();
  });
});

test('both apps scroll a short page alike while editing', async ({ browser }) => {
  const pair = await Pair.open(browser, { width: 390, height: 300 });
  const scrolls = () => Promise.all(pair.pages.map((page) => page.evaluate(() => Math.round(scrollY))));
  const toBottom = () => pair.each((page) => page.evaluate(() => scrollTo(0, document.body.scrollHeight)));
  await pair.click('#fullName');
  await pair.press('Tab');
  // From the bottom: Tab shows the email's error above the view, which
  // stays put, then scrolls the password into it; typing scrolls back to
  // the caret, and a key that moves nothing does not.
  for (const key of ['Tab', 'End', 'x']) {
    await toBottom();
    await pair.press(key);
    const [react, wasm] = await scrolls();
    expect(wasm, key).toBe(react);
  }
  await pair.close();
});

test('the canvas app is one canvas that takes all input', async ({ page }) => {
  await open(page, APPS[1].url);
  expect(await page.evaluate(() => [...document.body.querySelectorAll('*')].map((element) => element.tagName))).toEqual([
    'MAIN',
    'CANVAS',
    'SCRIPT',
  ]);
  const canvas = page.getByRole('main').locator('canvas');
  await expect(canvas).toHaveAttribute('tabindex', '0');
  await expect(canvas).toHaveAccessibleName('Create your account');
  await expect(page.locator('input, button, label, select, textarea')).toHaveCount(0);
});
