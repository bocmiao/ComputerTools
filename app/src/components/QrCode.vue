<script setup lang="ts">
import { computed } from 'vue'
import { encodeQr } from '../utils/qrcode'
import { QR_QUIET, qrPixels } from '../utils/textQr'

// 把文字画成二维码（SVG，四周留白边）。码越密画得越大，窄的时候缩到放得下。
// 内容可能是 WiFi 密码：不放进 title、aria-label 这类属性，只在图里。
const props = defineProps<{ text: string; label: string }>()

const code = computed(() => {
  try {
    return encodeQr(props.text)
  } catch {
    return null
  }
})

/** 所有黑格合成一条路径 */
const path = computed(() => {
  const q = code.value
  if (!q) return ''
  let d = ''
  q.modules.forEach((row, y) => {
    row.forEach((dark, x) => {
      if (dark) d += `M${x + QR_QUIET},${y + QR_QUIET}h1v1h-1z`
    })
  })
  return d
})
const box = computed(() => (code.value ? code.value.size + QR_QUIET * 2 : 0))
const width = computed(() => (code.value ? `${qrPixels(code.value.size)}px` : '0'))
</script>

<template>
  <svg
    v-if="code"
    class="qr"
    :style="{ width }"
    :viewBox="`0 0 ${box} ${box}`"
    role="img"
    :aria-label="label"
    shape-rendering="crispEdges"
  >
    <rect width="100%" height="100%" fill="#fff" />
    <path :d="path" fill="#000" />
  </svg>
  <p v-else class="small danger-text">内容太长，画不成二维码。</p>
</template>

<style scoped>
/* 不跟着深色主题变：扫码软件要黑格白底 */
.qr { max-width: 100%; height: auto; aspect-ratio: 1; display: block; border-radius: 4px; }
</style>
