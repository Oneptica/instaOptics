import { resolve } from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'electron-vite'

export default defineConfig({
  main: {
    build: {
      rollupOptions: {
        // The compute entry runs in an Electron utility process and loads the Rust module.
        input: { index: resolve('src/main/index.ts'), compute: resolve('src/compute/index.ts') },
      },
    },
  },
  preload: {},
  renderer: {
    plugins: [react()],
  },
})
