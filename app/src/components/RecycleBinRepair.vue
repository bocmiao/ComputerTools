<script setup lang="ts">
import { nextTick, onMounted, ref, useTemplateRef } from 'vue'
import { isTauri, recycleDrives, recycleRepair } from '../api'
import type { RecycleDriveView, RecycleRepairView } from '../api/types'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'
import ConfirmDialog from './ConfirmDialog.vue'

// 回收站坏了：列出各个盘的回收站里有多少东西（后端只给个数和大小），选一个盘清空并重建：删掉那个盘的 $Recycle.Bin，
// 重启以后 Windows 重新建一个（照微软《The Recycle Bin is corrupted》，后端 medkit_core::recycle_bin）。
// 删了找不回来，而且是这台电脑上所有账户的，所以先问一下。

const drives = ref<RecycleDriveView[]>([])
const loading = ref(false)
const error = ref('')
const confirming = ref<RecycleDriveView | null>(null)
const running = ref(false)
const outcome = ref<{ ok: boolean; text: string } | null>(null)
const message = useTemplateRef<HTMLElement>('message')

function driveName(d: RecycleDriveView): string {
  const kind = d.system ? '系统盘' : d.removable ? 'U 盘、移动硬盘' : '本地磁盘'
  return `${d.letter}:${d.label ? ` ${d.label}` : ''}（${kind}）`
}

function contents(d: RecycleDriveView): string {
  if (!d.exists) return '没有回收站文件夹，不用清空（往这个盘的回收站里删东西时 Windows 会建）。'
  if (!d.complete && d.files === 0) return '没来得及数（盘比较慢，或者东西很多）。'
  if (d.files === 0) return '回收站是空的。'
  if (!d.complete) return `回收站里至少有 ${d.files} 个文件、${formatBytes(d.bytes)}（东西太多，没数完）。`
  return `回收站里有 ${d.files} 个文件，共 ${formatBytes(d.bytes)}。`
}

function outcomeText(r: RecycleRepairView): { ok: boolean; text: string } {
  if (r.outcome === 'done') {
    return {
      ok: true,
      text: `已经删掉了 ${r.letter} 盘的回收站。请重启电脑，Windows 会重新建好回收站；重启以后往回收站里删一个没用的文件试试。`,
    }
  }
  if (r.outcome === 'partly') {
    return {
      ok: false,
      text: `${r.letter} 盘的回收站里还有 ${r.left} 个文件删不掉（可能正被别的程序用着）。请重启电脑以后再来清空一次。`,
    }
  }
  return { ok: true, text: `${r.letter} 盘上本来就没有回收站文件夹，不用清空。` }
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    drives.value = await recycleDrives()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

function ask(d: RecycleDriveView): void {
  if (running.value) return
  outcome.value = null
  confirming.value = d
}

async function repair(): Promise<void> {
  const d = confirming.value
  if (!d || running.value) return
  running.value = true
  let next: { ok: boolean; text: string }
  try {
    next = outcomeText(await recycleRepair(d.letter))
  } catch (e) {
    next = { ok: false, text: errorText(e) }
  }
  running.value = false
  // 先关确认框（焦点回到按钮上），再显示结果，焦点交给结果那句话
  confirming.value = null
  await nextTick()
  outcome.value = next
  await nextTick()
  message.value?.focus()
  void load()
}

onMounted(load)
</script>

<template>
  <section class="card recycle-card" aria-labelledby="recycle-title">
    <h2 id="recycle-title" class="section-title">清空并重建回收站</h2>
    <p class="muted small">
      点了「是」以后还是一再提示「回收站已损坏」，或者回收站打不开、删东西时出错，就用这里：照微软的办法把那个盘上的回收站文件夹（$Recycle.Bin）整个删掉，重启电脑以后 Windows 会重新建一个好的。
    </p>
    <p class="banner banner-warning small" role="note">
      回收站里的东西会永久删除，找不回来；这台电脑上每个账户放进这个盘回收站的都会删掉。有要留的，先打开回收站，右键「还原」出来再清空。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里是演示：列的是演示的盘，不会真的删。</p>
    <p v-if="loading" class="loading-line small" role="status">
      <BusySpinner size="small" />正在数各个盘的回收站里有多少东西…
    </p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <ul v-if="drives.length" class="drives">
      <li v-for="d in drives" :key="d.letter" class="drive">
        <div class="drive-main">
          <p class="name">{{ driveName(d) }}</p>
          <p class="muted small">{{ contents(d) }}</p>
        </div>
        <button
          type="button"
          class="btn btn-secondary btn-small"
          :disabled="running || loading || !d.exists"
          @click="ask(d)"
        >
          清空并重建
        </button>
      </li>
    </ul>
    <p v-else-if="!loading && !error" class="muted small">没有找到这台电脑上的盘。</p>
    <div>
      <button type="button" class="btn btn-ghost btn-small" :disabled="running || loading" @click="load">刷新列表</button>
    </div>
    <div v-if="outcome" :role="outcome.ok ? 'status' : 'alert'">
      <p ref="message" class="small outcome" :class="outcome.ok ? 'success-text' : 'danger-text'" tabindex="-1">
        {{ outcome.text }}
      </p>
    </div>

    <ConfirmDialog
      v-if="confirming"
      :title="`清空并重建 ${confirming.letter} 盘的回收站？`"
      confirm-text="清空并重建"
      :busy="running"
      @confirm="repair"
      @close="confirming = null"
    >
      <p>{{ confirming.letter }} 盘：{{ contents(confirming) }}</p>
      <p>回收站里的东西会永久删除，找不回来；这台电脑上每个账户放进 {{ confirming.letter }} 盘回收站的都会删掉。有要留的，先取消，打开回收站右键「还原」出来。</p>
      <p>删完以后重启电脑，Windows 会重新建好回收站。</p>
    </ConfirmDialog>
  </section>
</template>

<style scoped>
.recycle-card { display: flex; flex-direction: column; gap: 9px; }
.drives { display: flex; flex-direction: column; gap: 6px; padding: 0; list-style: none; }
.drive {
  display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center; justify-content: space-between;
  padding: 9px 12px; border-radius: var(--radius); background: var(--color-surface-2);
}
.drive-main { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.name { font-weight: 600; }
.outcome { overflow-wrap: anywhere; }
.outcome:focus { outline: none; }
</style>
