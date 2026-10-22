import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: { '/signup.v1.SignupService': 'http://127.0.0.1:50051' },
  },
  build: { outDir: 'dist/client' },
  test: {
    environment: 'jsdom',
    setupFiles: ['src/test-setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      include: ['src/**', 'scripts/**'],
      exclude: ['src/gen/**', 'src/generated/**', 'src/**/*.test.*', 'src/test-setup.ts'],
    },
  },
});
