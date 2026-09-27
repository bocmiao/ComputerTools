<script setup lang="ts">
import { ref } from 'vue'
import { contextMenuList, contextMenuSet } from '../api'
import type { ContextMenuItem } from '../api/types'
import { errorText } from '../utils/format'
import BusySpinner from './BusySpinner.vue'
import ExplorerRestart from './ExplorerRestart.vue'
import TagPill from './TagPill.vue'

// 右键菜单里软件加的项目：列表要查每个软件的签名、读应用清单，要好几秒，所以点了才列。
// 只列第三方的（Windows 自带的不列）。拿掉只是不在菜单里显示，软件本身不受影响；每次改动都记进修改日志。
// 菜单命令下次右键就生效；外壳扩展和 Windows 11 新菜单里应用的项目要重启资源管理器，改完给「现在重启资源管理器」。

const items = ref<ContextMenuItem[]>([])
const loaded = ref(false)
const loading = ref(false)
const busy = ref<string | null>(null)
const error = ref<string | null>(null)
const notice = ref<string | null>(null)
/** 改过要重启资源管理器才生效的项目 */
const needsRestart = ref(false)

async function load(): Promise<void> {
  if (loading.value || busy.value) return
  loading.value = true
  error.value = null
  try {
    items.value = await contextMenuList()
    loaded.value = true
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

async function toggle(item: ContextMenuItem): Promise<void> {
  if (busy.value) return
  busy.value = item.id
  error.value = null
  notice.value = null
  try {
    const r = await contextMenuSet(item.id, !item.visible)
    notice.value = `「${item.title}」${r.message}`
    if (!r.ok) {
      error.value = r.error
      return
    }
    if (r.entryIds.length > 0 && r.reboot === 'explorer') needsRestart.value = true
    items.value = await contextMenuList()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = null
  }
}

/** 发布者和签名：有签名写发布者；没签名、签名无效的写明，免得被冒名 */
function publisher(item: ContextMenuItem): string {
  const who = item.publisher ?? '发布者不明'
  switch (item.signature) {
    case 'valid':
      return item.publisher ?? '有数字签名'
    case 'unsigned':
      return `${who}（没有数字签名）`
    case 'invalid':
      return `${who}（数字签名无效）`
    default:
      return who
  }
}

const kindLabel: Record<ContextMenuItem['kind'], string> = {
  command: '菜单命令',
  extension: '软件扩展',
  app: 'Windows 11 新菜单',
}
</script>

<template>
  <section class="group" aria-labelledby="settings-context-menu">
    <h2 id="settings-context-menu" class="section-title">右键菜单里的软件项目</h2>
    <div class="card menu-card">
      <p class="small">
        软件装好以后常往右键菜单里加东西（压缩、网盘、杀毒、编辑器……），多了找起来费劲。这里列出软件加的项目（Windows 自带的不列），不常用的可以拿掉。拿掉只是不在菜单里显示，软件本身照常能用；每一次改动都记在「修改日志」里，随时能恢复。
      </p>
      <p class="muted small">
        Windows 11 的右键菜单分两层：软件的老式项目在「显示更多选项」里。想让它们直接出现在第一层，可以用上面「资源管理器」里的「经典右键菜单」。
      </p>
      <div class="row">
        <button type="button" class="btn btn-secondary btn-small" :disabled="loading || !!busy" @click="load">
          {{ loaded ? '刷新列表' : '列出右键菜单里的软件项目' }}
        </button>
      </div>
      <p v-if="loading" class="loading-line" role="status"><BusySpinner size="small" />正在读取右键菜单（要查每个软件的签名，可能要十几秒）…</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
      <p v-if="notice" class="small" role="status">{{ notice }}</p>
      <ExplorerRestart v-if="needsRestart" />
      <p v-if="loaded && !loading && !error && items.length === 0" class="muted small">右键菜单里没有软件加的项目。</p>
      <ul v-if="items.length" class="list">
        <li v-for="item in items" :key="item.id" class="entry">
          <div class="entry-main">
            <p class="entry-title">
              {{ item.title }}
              <TagPill :tone="item.visible ? 'info' : 'neutral'">{{ item.visible ? '显示' : '已拿掉' }}</TagPill>
              <TagPill tone="neutral">{{ kindLabel[item.kind] }}</TagPill>
              <TagPill v-if="item.shiftOnly && item.visible" tone="neutral">按住 Shift 右键才显示</TagPill>
            </p>
            <p class="muted small">
              {{ publisher(item) }} · 右键{{ item.scopes.join('、') }}时出现<template v-if="item.location"> · {{ item.location }}</template>
            </p>
            <p v-if="item.note" class="small">{{ item.note }}</p>
            <p v-if="item.path" class="muted small path" :title="item.path">{{ item.path }}</p>
          </div>
          <button type="button" class="btn btn-secondary btn-small" :disabled="!!busy || loading" @click="toggle(item)">
            <BusySpinner v-if="busy === item.id" size="small" />{{ item.visible ? '拿掉' : '恢复' }}
          </button>
        </li>
      </ul>
    </div>
  </section>
</template>

<style scoped>
.group { display: flex; flex-direction: column; gap: 10px; }
.menu-card { display: flex; flex-direction: column; gap: 8px; }
.row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.list { display: flex; flex-direction: column; gap: 8px; list-style: none; margin-top: 4px; }
.entry {
  display: flex; align-items: center; justify-content: space-between; gap: 16px;
  padding: 10px 14px; border-radius: var(--radius); background: var(--color-surface-2);
}
.entry > .btn { flex: none; }
.entry-main { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.entry-title { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-weight: 600; }
.path { overflow-wrap: anywhere; }
</style>
