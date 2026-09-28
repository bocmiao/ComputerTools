<script setup lang="ts">
import { ref } from 'vue'
import { shellPlacesList, shellPlacesSet } from '../api'
import type { ShellPlaceItem } from '../api/types'
import { errorText } from '../utils/format'
import BusySpinner from './BusySpinner.vue'
import ExplorerRestart from './ExplorerRestart.vue'
import TagPill from './TagPill.vue'

// 资源管理器里软件加的图标：左边导航栏最上面一层（和「此电脑」「网络」并列的网盘、OneDrive）和「此电脑」里的
// （WPS 云文档、百度网盘这些）。Windows 自己的（此电脑、网络、回收站、主文件夹、库、用户文件夹）不列；OneDrive、
// 图库、3D 对象这几个 Windows 自带的列出来，标明。隐藏只是不显示，软件本身不受影响；每次改动都记进修改日志。
// 已经开着的资源管理器窗口不一定马上变，改完给「现在重启资源管理器」。点了才列。

const items = ref<ShellPlaceItem[]>([])
const loaded = ref(false)
const loading = ref(false)
const busy = ref<string | null>(null)
const error = ref<string | null>(null)
const notice = ref<string | null>(null)
/** 改过以后，给「现在重启资源管理器」 */
const needsRestart = ref(false)

async function load(): Promise<void> {
  if (loading.value || busy.value) return
  loading.value = true
  error.value = null
  try {
    items.value = await shellPlacesList()
    loaded.value = true
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

async function toggle(item: ShellPlaceItem): Promise<void> {
  if (busy.value) return
  busy.value = item.id
  error.value = null
  notice.value = null
  try {
    const r = await shellPlacesSet(item.id, !item.visible)
    notice.value = `「${item.title}」${r.message}`
    if (!r.ok) {
      error.value = r.error
      return
    }
    if (r.entryIds.length > 0 && r.reboot === 'explorer') needsRestart.value = true
    items.value = await shellPlacesList()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = null
  }
}

const placeLabel: Record<ShellPlaceItem['place'], string> = {
  nav: '在左边的导航栏里',
  pc: '在「此电脑」里',
}
</script>

<template>
  <section class="group" aria-labelledby="settings-shell-places">
    <h2 id="settings-shell-places" class="section-title">资源管理器里多出来的图标</h2>
    <div class="card menu-card">
      <p class="small">
        装了网盘、WPS 这些软件以后，资源管理器左边的导航栏和「此电脑」里常多出几个图标（WPS 云文档、百度网盘、OneDrive……）。这里列出软件加的（Windows 自己的「此电脑」「网络」「回收站」这些不列），不用的可以隐藏。隐藏只是不显示，软件本身照常能用，里面的文件也都在；每一次改动都记在「修改日志」里，随时能恢复。
      </p>
      <p class="muted small">有的软件更新以后会把自己的图标加回来，到时候再隐藏一次就行。</p>
      <div class="row">
        <button type="button" class="btn btn-secondary btn-small" :disabled="loading || !!busy" @click="load">
          {{ loaded ? '刷新列表' : '列出资源管理器里的图标' }}
        </button>
      </div>
      <p v-if="loading" class="loading-line" role="status"><BusySpinner size="small" />正在读取资源管理器里的图标…</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
      <p v-if="notice" class="small" role="status">{{ notice }}</p>
      <ExplorerRestart v-if="needsRestart" />
      <p v-if="loaded && !loading && !error && items.length === 0" class="muted small">
        资源管理器的导航栏和「此电脑」里没有软件加的图标。
      </p>
      <ul v-if="items.length" class="list">
        <li v-for="item in items" :key="item.id" class="entry">
          <div class="entry-main">
            <p class="entry-title">
              {{ item.title }}
              <TagPill :tone="item.visible ? 'info' : 'neutral'">{{ item.visible ? '显示' : '已隐藏' }}</TagPill>
              <TagPill v-if="item.windowsOwn" tone="neutral">Windows 自带</TagPill>
            </p>
            <p class="muted small">{{ placeLabel[item.place] }}</p>
            <p v-if="item.note" class="small">{{ item.note }}</p>
          </div>
          <button type="button" class="btn btn-secondary btn-small" :disabled="!!busy || loading" @click="toggle(item)">
            <BusySpinner v-if="busy === item.id" size="small" />{{ item.visible ? '隐藏' : '恢复' }}
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
</style>
