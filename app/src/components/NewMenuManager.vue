<script setup lang="ts">
import { ref } from 'vue'
import { newMenuList, newMenuSet } from '../api'
import type { NewMenuItem } from '../api/types'
import { errorText } from '../utils/format'
import BusySpinner from './BusySpinner.vue'
import TagPill from './TagPill.vue'

// 右键「新建」菜单里的项：软件装好以后常往里加（新建 Word 文档、WPS 表格、思维导图……），Windows 自带的
// 位图图像、联系人这些很多人也用不上。关掉只是不在菜单里显示，软件本身照常能用；每次改动都记进修改日志。
// 「文件夹」「快捷方式」是基本功能，不列。点了才列（要把注册表里的文件类型都看一遍）。

const items = ref<NewMenuItem[]>([])
const loaded = ref(false)
const loading = ref(false)
const busy = ref<string | null>(null)
const error = ref<string | null>(null)
const notice = ref<string | null>(null)

async function load(): Promise<void> {
  if (loading.value || busy.value) return
  loading.value = true
  error.value = null
  try {
    items.value = await newMenuList()
    loaded.value = true
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

async function toggle(item: NewMenuItem): Promise<void> {
  if (busy.value) return
  busy.value = item.id
  error.value = null
  notice.value = null
  try {
    const r = await newMenuSet(item.id, !item.visible)
    notice.value = `「${item.title}」${r.message}`
    if (!r.ok) {
      error.value = r.error
      return
    }
    items.value = await newMenuList()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = null
  }
}
</script>

<template>
  <section class="group" aria-labelledby="settings-new-menu">
    <h2 id="settings-new-menu" class="section-title">右键「新建」菜单</h2>
    <div class="card menu-card">
      <p class="small">
        在桌面或文件夹空白处点右键，「新建」里的项目多了找起来费劲：装 WPS、Office、思维导图这些软件时常往里加，Windows 自带的「BMP 图像」「联系人」很多人也用不上。这里列出「新建」里的项目（「文件夹」「快捷方式」除外），不用的可以关掉。关掉只是不在菜单里显示，软件本身照常能用；每一次改动都记在「修改日志」里，随时能恢复。
      </p>
      <div class="row">
        <button type="button" class="btn btn-secondary btn-small" :disabled="loading || !!busy" @click="load">
          {{ loaded ? '刷新列表' : '列出「新建」菜单里的项目' }}
        </button>
      </div>
      <p v-if="loading" class="loading-line" role="status"><BusySpinner size="small" />正在读取「新建」菜单…</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
      <p v-if="notice" class="small" role="status">{{ notice }}</p>
      <p v-if="loaded && !loading && !error && items.length === 0" class="muted small">「新建」菜单里除了文件夹和快捷方式，没有别的项目。</p>
      <ul v-if="items.length" class="list">
        <li v-for="item in items" :key="item.id" class="entry">
          <div class="entry-main">
            <p class="entry-title">
              {{ item.title }}
              <TagPill :tone="item.visible ? 'info' : 'neutral'">{{ item.visible ? '显示' : '已关掉' }}</TagPill>
              <TagPill v-if="item.windowsOwn" tone="neutral">Windows 自带</TagPill>
            </p>
            <p class="muted small">新建的是 {{ item.ext }} 文件 · {{ item.location }}</p>
          </div>
          <button type="button" class="btn btn-secondary btn-small" :disabled="!!busy || loading" @click="toggle(item)">
            <BusySpinner v-if="busy === item.id" size="small" />{{ item.visible ? '关掉' : '恢复' }}
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
