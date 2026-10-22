import { defineConfig, devices } from '@playwright/test';

// Runs against the Kind deployment (`make deploy`); override with REACT_URL and WASM_URL.
export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  forbidOnly: true,
  reporter: [['list']],
  use: { ...devices['Desktop Chrome'], trace: 'retain-on-failure' },
});
