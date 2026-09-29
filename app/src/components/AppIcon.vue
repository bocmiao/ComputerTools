<script lang="ts">
// 手画的线条图标，不依赖任何图标库。24×24，只用线条（stroke）。
// 一条 path 画得完的放在 PATHS 里；要用圆、矩形的写在模板里。

/** 一条 path 画完的图标 */
const PATHS = {
  wifi: 'M2 9a15 15 0 0 1 20 0M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0M12 19.5h.01',
  power: 'M12 3v8M6.5 6.5a8 8 0 1 0 11 0',
  window: 'M4 5h16v14H4zM4 9h16M7 7h.01M10 7h.01',
  folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
  printer: 'M7 8V4h10v4M5 8h14v8h-2M5 8v8h2M7 13h10v7H7z',
  monitor: 'M3 5h18v11H3zM8 20h8M12 16v4',
  layout: 'M3 4h18v16H3zM3 16h18M6 18h3',
  keyboard: 'M3 7h18v10H3zM7 11h.01M11 11h.01M15 11h.01M7 14h10',
  speaker: 'M4 10h4l5-4v12l-5-4H4zM17 9a4 4 0 0 1 0 6',
  hash: 'M9 4 7 20M17 4l-2 16M4 9h16M3 15h16',
  text: 'M5 6h14M12 6v13M9 19h6',
  braces:
    'M9 4c-2 0-3 1-3 3v2c0 1-1 2-2 3 1 1 2 2 2 3v2c0 2 1 3 3 3M15 4c2 0 3 1 3 3v2c0 1 1 2 2 3-1 1-2 2-2 3v2c0 2-1 3-3 3',
  swap: 'M4 8h12M12 4l4 4-4 4M20 16H8M12 12l-4 4 4 4',
  diff: 'M8 4v16M16 4v16M4 8h8M12 16h8',
  qr: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h2v2h-2zM18 18h2v2h-2z',
  money: 'M4 7h16v10H4zM12 10v4M9 12h6',
  calendar: 'M4 6h16v14H4zM4 10h16M8 3v5M16 3v5',
  image: 'M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4M15 9h.01',
  pdf: 'M7 3h7l5 5v13H7zM14 3v5h5M10 13h6M10 17h4',
  'long-image': 'M7 3h10v18H7zM7 9h10M7 15h10',
  'scan-text': 'M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4M8 10h8M8 14h5',
  'eye-off':
    'M3 3l18 18M10.5 6.2A10 10 0 0 1 22 12a14 14 0 0 1-3 3.5M6.6 6.6A14 14 0 0 0 2 12s4 7 10 7a9.6 9.6 0 0 0 4.4-1',
  rename: 'M4 20h4L19 9l-4-4L4 16z',
  'file-lock': 'M7 3h7l5 5v13H7zM14 3v5h5M10 15h5v4h-5zM11 15v-1.5a1.5 1.5 0 0 1 3 0V15',
  popup: 'M4 4h16v12H8l-4 4z',
  'search-file': 'M13 3H7v18h5M13 3l5 5v2M13 3v5h5M17 12a3 3 0 1 0 0 6a3 3 0 1 0 0-6zM21 20l-1.8-1.8',
  chat: 'M4 5h16v11H9l-5 4z',
  usb: 'M12 3v14M9 6l3-3 3 3M8 11h8M12 17a2 2 0 1 0 0 4a2 2 0 1 0 0-4z',
  gauge: 'M4 16a8 8 0 1 1 16 0M12 16l4-5',
  sun: 'M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8zM12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5',
  coffee: 'M5 9h11v5a5 5 0 0 1-5 5h-1a5 5 0 0 1-5-5zM16 10h2a2 2 0 0 1 0 4h-2M9 3v3M13 3v3',
  clock: 'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 7v5l3 2',
  info: 'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 11v5M12 8h.01',
  help: 'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .8-1 1.5v.7M12 17h.01',
  refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5',
  'chevron-right': 'm9 6 6 6-6 6',
  'chevron-up': 'm6 15 6-6 6 6',
  search: 'M11 4.5a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM20 20l-4.2-4.2',
  bolt: 'M13 3 5 14h6l-1 7 8-11h-6z',
  cpu: 'M7 7h10v10H7zM10 10h4v4h-4zM9 3v4M15 3v4M9 17v4M15 17v4M3 9h4M3 15h4M17 9h4M17 15h4',
  disk: 'M4 6h16v12H4zM4 14h16M16 16.5h.01',
  shield: 'M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z',
  download: 'M12 4v11M7 10l5 5 5-5M5 20h14',
  lock: 'M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 0 1 7 0v3',
  grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
  mouse: 'M7 9a5 5 0 0 1 10 0v6a5 5 0 0 1-10 0zM12 4v5',
  globe: 'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18',
  'arrow-right': 'M5 12h14M13 6l6 6-6 6',
  camera: 'M4 8h4l2-3h4l2 3h4v11H4zM12 11a3 3 0 1 0 0 6a3 3 0 1 0 0-6z',
  play: 'M8 5v14l11-7z',
} as const

export type IconName =
  | 'logo'
  | 'health'
  | 'symptoms'
  | 'settings'
  | 'tools'
  | 'journal'
  | 'report'
  | 'close'
  | 'warning'
  | 'check'
  | 'back'
  | 'chevron-down'
  | 'copy'
  | 'undo'
  | 'external'
  | keyof typeof PATHS
</script>

<script setup lang="ts">
withDefaults(defineProps<{ name: IconName; size?: number }>(), { size: 20 })

function pathOf(name: IconName): string {
  return (PATHS as Record<string, string>)[name] ?? ''
}
</script>

<template>
  <svg
    class="icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <template v-if="name === 'logo'">
      <rect x="3" y="7" width="18" height="13" rx="2.5" />
      <path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7" />
      <path d="M12 10.5v6M9 13.5h6" />
    </template>
    <path v-else-if="name === 'health'" d="M3 12h4l2.5-6 5 12 2.5-6h4" />
    <template v-else-if="name === 'symptoms'">
      <circle cx="11" cy="11" r="6.5" />
      <path d="m20 20-4.2-4.2" />
    </template>
    <template v-else-if="name === 'settings'">
      <path d="M4 7h9M17 7h3M4 17h3M11 17h9" />
      <circle cx="15" cy="7" r="2" />
      <circle cx="9" cy="17" r="2" />
    </template>
    <!-- 扳手：开口朝右上 -->
    <path
      v-else-if="name === 'tools'"
      transform="translate(-0.5 0.5) rotate(45 12 12)"
      d="M10.3 2.09A5.2 5.2 0 0 0 10 11.8V20A2 2 0 0 0 14 20V11.8A5.2 5.2 0 0 0 13.7 2.09V5.6A1.7 1.7 0 0 1 10.3 5.6Z"
    />
    <template v-else-if="name === 'journal'">
      <path d="M9 6h11M9 12h11M9 18h11" />
      <circle cx="4.5" cy="6" r="1" fill="currentColor" stroke="none" />
      <circle cx="4.5" cy="12" r="1" fill="currentColor" stroke="none" />
      <circle cx="4.5" cy="18" r="1" fill="currentColor" stroke="none" />
    </template>
    <template v-else-if="name === 'report'">
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5M9 13h6M9 17h6" />
    </template>
    <path v-else-if="name === 'close'" d="M6 6l12 12M18 6 6 18" />
    <template v-else-if="name === 'warning'">
      <path d="M10.3 4.4 2.8 17.5A2 2 0 0 0 4.5 20.5h15a2 2 0 0 0 1.7-3L13.7 4.4a2 2 0 0 0-3.4 0z" />
      <path d="M12 9.5v4.5M12 17h.01" />
    </template>
    <path v-else-if="name === 'check'" d="m5 12.5 4.5 4.5L19 7.5" />
    <path v-else-if="name === 'back'" d="M15 5l-7 7 7 7" />
    <path v-else-if="name === 'chevron-down'" d="m6 9 6 6 6-6" />
    <template v-else-if="name === 'copy'">
      <rect x="8" y="8" width="12" height="12" rx="2" />
      <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
    </template>
    <template v-else-if="name === 'undo'">
      <path d="M9 14 4 9l5-5" />
      <path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11" />
    </template>
    <!-- 在新窗口里打开 -->
    <template v-else-if="name === 'external'">
      <path d="M14 4h6v6" />
      <path d="m20 4-8.5 8.5" />
      <path d="M19 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h4" />
    </template>
    <path v-else :d="pathOf(name)" />
  </svg>
</template>

<style scoped>
.icon {
  flex: none;
}
</style>
