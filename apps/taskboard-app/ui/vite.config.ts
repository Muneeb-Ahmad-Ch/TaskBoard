import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

// Tauri expects a fixed dev port and must see its own errors in the terminal.
export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  build: {
    // WebView2 is evergreen Chromium.
    target: 'chrome120',
    sourcemap: false,
  },
})
