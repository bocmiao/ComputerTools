<script setup lang="ts">
import { ref, watch } from 'vue'
import { isTauri, renameApply, renamePreview, renameSelectFolder } from '../api'

const folder = ref('')
const prefix = ref('整理_')
const entries = ref<{ source: string; target: string }[]>([])
const busy = ref(false)
const error = ref('')
const message = ref('')

watch(prefix, () => { entries.value = []; error.value = ''; message.value = '' })

async function selectFolder(): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  message.value = ''
  try {
    const selected = await renameSelectFolder()
    if (selected) {
      folder.value = selected
      entries.value = []
    }
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function preview(): Promise<void> {
  if (busy.value || !folder.value) return
  busy.value = true
  entries.value = []
  error.value = ''
  message.value = ''
  try {
    const result = await renamePreview(prefix.value)
    entries.value = result.entries
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function apply(): Promise<void> {
  if (busy.value || !entries.value.length) return
  busy.value = true
  error.value = ''
  try {
    const count = await renameApply()
    entries.value = []
    message.value = `已重命名 ${count} 个文件。`
  } catch (e) {
    entries.value = []
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <article class="card rename-card">
    <h3 class="section-title">批量重命名文件</h3>
    <p class="muted small">选择文件夹，按名称排序后加前缀与序号，保留扩展名。只处理当前文件夹中的普通文件，不进入子文件夹。执行前可逐项预览。</p>
    <p v-if="!isTauri()" class="muted small">浏览器中显示演示文件；在 Windows 桌面程序中处理真实文件。</p>
    <button type="button" class="btn btn-secondary btn-small self-start" :disabled="busy" @click="selectFolder">选择文件夹</button>
    <p v-if="folder" class="muted small path">{{ folder }}</p>
    <label class="field-label" for="rename-prefix">新文件名前缀</label>
    <input id="rename-prefix" v-model="prefix" class="input" type="text" maxlength="80" placeholder="例如 旅行_" />
    <button type="button" class="btn btn-secondary btn-small self-start" :disabled="busy || !folder" @click="preview">预览重命名</button>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <p v-if="message" class="success-text small" role="status">{{ message }}</p>
    <template v-if="entries.length">
      <p class="muted small" role="status">将重命名 {{ entries.length }} 个文件。请核对后再执行；原文件名会改变。</p>
      <div class="preview-list">
        <table>
          <thead><tr><th scope="col">原文件名</th><th scope="col">新文件名</th></tr></thead>
          <tbody><tr v-for="entry in entries" :key="entry.source"><td>{{ entry.source }}</td><td>{{ entry.target }}</td></tr></tbody>
        </table>
      </div>
      <button type="button" class="btn btn-secondary btn-small self-start" :disabled="busy" @click="apply">确认重命名</button>
    </template>
  </article>
</template>

<style scoped>
.rename-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.self-start { align-self: flex-start; }
.path { overflow-wrap: anywhere; }
.preview-list { max-height: 260px; overflow: auto; border: 1px solid var(--color-border-strong); border-radius: var(--radius); }
table { width: 100%; border-collapse: collapse; text-align: left; font-size: var(--text-small); }
td, th { padding: 6px 9px; border-bottom: 1px solid var(--color-border-strong); overflow-wrap: anywhere; }
td { max-width: 180px; }
</style>
