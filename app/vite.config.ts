import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// 界面由 Tauri 加载：开发时连 http://localhost:5173，发布时读 dist/ 里的文件。
export default defineConfig({
  base: './',
  plugins: [vue()],
  // 让 `tauri dev` 的 Rust 编译输出不被 Vite 清屏
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    // Windows 上的 WebView2 基于 Chromium
    target: 'chrome105',
  },
})
