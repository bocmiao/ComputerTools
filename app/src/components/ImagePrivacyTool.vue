<script setup lang="ts">
import { ref } from 'vue'

const image = ref<File | null>(null)
const busy = ref(false)
const error = ref('')
const notice = ref('')

function selectImage(event: Event): void {
  image.value = (event.target as HTMLInputElement).files?.[0] ?? null
  error.value = ''
  notice.value = ''
}

async function exportCleanCopy(): Promise<void> {
  const file = image.value
  if (!file || busy.value) return
  busy.value = true
  error.value = ''
  notice.value = ''
  let bitmap: ImageBitmap | null = null
  try {
    if (!['image/jpeg', 'image/png', 'image/webp'].includes(file.type)) {
      throw new Error('请选择 JPG、PNG 或 WebP 图片。')
    }
    bitmap = await createImageBitmap(file)
    if (bitmap.width * bitmap.height > 40_000_000) {
      throw new Error('图片超过 4000 万像素，暂时无法在工具箱中处理。')
    }
    const canvas = document.createElement('canvas')
    canvas.width = bitmap.width
    canvas.height = bitmap.height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('当前系统无法创建图片处理画布。')
    context.drawImage(bitmap, 0, 0)
    const output = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
    if (!output) throw new Error('没能生成 PNG 图片。')
    const url = URL.createObjectURL(output)
    const link = document.createElement('a')
    link.href = url
    link.download = `${file.name.replace(/\.[^.]+$/, '')}-clean.png`
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 30_000)
    notice.value = `已生成 ${bitmap.width} × ${bitmap.height} 的 PNG 副本。原图没有改动。`
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : '图片处理失败。'
  } finally {
    bitmap?.close()
    busy.value = false
  }
}
</script>

<template>
  <article class="card image-tool">
    <h3 class="section-title">图片隐私清理</h3>
    <p class="muted small">把 JPG、PNG 或 WebP 重新导出为 PNG，去掉原图里的位置、拍摄设备等元数据。新图片可能比原图更大。</p>
    <label class="field-label" for="privacy-image">选择图片</label>
    <input id="privacy-image" type="file" accept="image/jpeg,image/png,image/webp" @change="selectImage" />
    <button type="button" class="btn btn-secondary btn-small export-button" :disabled="!image || busy" @click="exportCleanCopy">
      {{ busy ? '正在处理…' : '导出清理后的副本' }}
    </button>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <p v-if="notice" class="success-text small" role="status">{{ notice }}</p>
  </article>
</template>

<style scoped>
.image-tool { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.export-button { align-self: flex-start; }
</style>
