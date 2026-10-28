/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('.', import.meta.url));

// 多页面：每个 Tauri 窗口一个 HTML 入口
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: '127.0.0.1' },
  oxc: { jsx: { runtime: 'automatic', importSource: 'preact' } },
  build: {
    target: 'es2022',
    sourcemap: false,
    modulePreload: { polyfill: false },
    rollupOptions: {
      input: {
        pet: `${root}pet.html`,
        chat: `${root}chat.html`,
        settings: `${root}settings.html`,
        menu: `${root}menu.html`,
      },
    },
  },
  test: {
    environment: 'happy-dom',
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
