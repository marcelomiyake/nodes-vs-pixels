import { expect, test } from '@playwright/test';

import { ADA, APPS, fillValid, open, submit } from './apps';

for (const app of APPS) {
  test.describe(app.name, () => {
    test('flags every field and focuses the first on an empty submit', async ({ page }) => {
      await open(page, app.url);
      await submit(page);
      await expect(page.locator('#fullName')).toBeFocused();
      await expect(page.locator('#fullName')).toHaveAttribute('aria-invalid', 'true');
      await expect(page.locator('#fullName-message')).toHaveText('Enter your full name.');
      await expect(page.locator('#acceptTerms-message')).toHaveText('Accept the terms to continue.');
    });

    test('checks a field when it loses focus', async ({ page }) => {
      await open(page, app.url);
      await page.locator('#password').fill('short');
      await page.locator('#confirmPassword').focus();
      await expect(page.locator('#password-message')).toHaveText('Password must be at least 12 characters.');
      await page.locator('#password').fill(ADA.password);
      await expect(page.locator('#password-message')).toHaveText(
        'Use upper and lowercase letters, a number and a symbol.',
      );
    });

    test('signs up through the backend over gRPC-web', async ({ page }) => {
      await open(page, app.url);
      await fillValid(page);
      const response = page.waitForResponse('**/signup.v1.SignupService/SignUp');
      await submit(page);
      const call = await response;
      expect(call.status()).toBe(200);
      expect(call.request().headers()['content-type']).toBe('application/grpc-web+proto');
      expect(await call.headerValue('content-type')).toContain('application/grpc-web');
      const heading = page.getByRole('heading', { name: 'Account created' });
      await expect(heading).toBeFocused();
      await expect(page.getByText(`Welcome aboard, ${ADA.fullName}. Your account is ready.`)).toBeVisible();
    });

    test('reports an unreachable server and allows a retry', async ({ page }) => {
      await open(page, app.url);
      await page.route('**/signup.v1.SignupService/**', (route) => route.abort());
      await fillValid(page);
      await submit(page);
      await expect(page.getByRole('alert')).toHaveText("We couldn't reach the server. Please try again.");
      await expect(page.locator('#fullName')).toBeEditable();
      await page.unroute('**/signup.v1.SignupService/**');
      await submit(page);
      await expect(page.getByRole('heading', { name: 'Account created' })).toBeVisible();
    });
  });
}

test('both apps expose the same accessibility tree', async ({ browser }) => {
  const trees = [];
  for (const app of APPS) {
    const page = await browser.newPage();
    await open(page, app.url);
    await submit(page);
    trees.push(await page.locator('main').ariaSnapshot());
    await page.close();
  }
  expect(trees[1]).toBe(trees[0]);
});
