import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['frontend/__tests__/**/*.test.js'],
  },
});
