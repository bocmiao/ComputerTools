<script setup lang="ts">
import { computed, ref } from 'vue'
import QrCode from './QrCode.vue'
import { QR_MAX_BYTES, qrHint, qrText, utf8Bytes } from '../utils/textQr'

// 文字、网址变二维码：边输入边画，只在这台电脑上生成
const input = ref('')
const text = computed(() => qrText(input.value))
const bytes = computed(() => utf8Bytes(text.value))
const hint = computed(() => qrHint(text.value))
const tooLong = computed(() => bytes.value > QR_MAX_BYTES)
</script>

<template>
  <article class="card qr-card">
    <h3 class="section-title">文字、网址变二维码</h3>
    <p class="muted small">
      把电脑上的网址、文字、取件码发到手机上：用手机相机或者微信「扫一扫」扫一下，就能打开网址或者复制文字。只在这台电脑上生成，不上传。
    </p>
    <label class="field-label" for="qr-input">网址或文字</label>
    <textarea
      id="qr-input"
      v-model="input"
      class="input text-box"
      rows="3"
      spellcheck="false"
      placeholder="例如 https://www.baidu.com"
    ></textarea>
    <p class="muted small">{{ bytes }} / {{ QR_MAX_BYTES }} 字节（一个汉字算 3 个字节，英文字母和数字算 1 个）</p>
    <p v-if="hint" :class="hint.tone === 'danger' ? 'danger-text' : 'muted'" class="small" role="status">{{ hint.text }}</p>
    <QrCode v-if="text && !tooLong" :text="text" label="刚才输入的内容的二维码" />
  </article>
</template>

<style scoped>
.qr-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.text-box { resize: vertical; line-height: 1.5; }
</style>
