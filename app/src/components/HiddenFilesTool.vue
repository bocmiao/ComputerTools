<script setup lang="ts">
import { computed, ref } from 'vue'
import { hiddenPickFolder, hiddenRescan, hiddenRestore, hiddenUndo, isTauri } from '../api'
import type { HiddenReport, HiddenRestore } from '../api/types'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'

// U 盘里的文件不见了：U 盘病毒把原来的文件、文件夹设成「隐藏」「系统」属性，再放一堆同名的快捷方式。
// 后端（src-tauri/src/hidden.rs）列出被藏起来的东西，勾选的去掉这两个属性（程序和脚本文件照样藏着），能撤销；
// 病毒放的快捷方式、藏着的程序只提醒，不删。U 盘只能在系统的选择框里选，系统盘不给用。

const report = ref<HiddenReport | null>(null)
const selected = ref<Set<number>>(new Set())
const busy = ref<'' | 'pick' | 'rescan' | 'restore' | 'undo'>('')
const error = ref('')
const done = ref<HiddenRestore['result'] | null>(null)
const undone = ref<{ restored: number; failed: number } | null>(null)

const selectedCount = computed(() => selected.value.size)
const nothingHidden = computed(
  () => !!report.value && !report.value.items.length && !report.value.programs.length && !report.value.shortcuts.length,
)

function show(next: HiddenReport | null): void {
  report.value = next
  // 默认全选：被藏起来的普通文件和文件夹，都该显示回来
  selected.value = new Set(next?.items.map((i) => i.id) ?? [])
}

async function run(kind: 'pick' | 'rescan', call: () => Promise<HiddenReport | null>): Promise<void> {
  if (busy.value) return
  busy.value = kind
  error.value = ''
  try {
    const next = await call()
    if (next || kind === 'rescan') {
      done.value = null
      undone.value = null
      show(next)
    }
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = ''
  }
}

function toggle(id: number): void {
  const next = new Set(selected.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  selected.value = next
}

async function restore(): Promise<void> {
  if (busy.value || !selectedCount.value) return
  busy.value = 'restore'
  error.value = ''
  undone.value = null
  try {
    const r = await hiddenRestore([...selected.value].sort((a, b) => a - b))
    done.value = r.result
    show(r.report)
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = ''
  }
}

async function undo(): Promise<void> {
  if (busy.value) return
  busy.value = 'undo'
  error.value = ''
  try {
    const r = await hiddenUndo()
    undone.value = r.result
    done.value = null
    show(r.report)
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = ''
  }
}

function describe(item: HiddenReport['items'][number]): string {
  if (!item.isDir) return `文件，${formatBytes(item.size)}`
  const more = item.countedAll ? '' : '多'
  if (!item.inside) return '文件夹，空的'
  return `文件夹，里面有 ${item.inside} 个${more}文件和文件夹${item.hiddenInside ? `（其中 ${item.hiddenInside} 个也被藏起来了，会一起显示出来，程序和脚本除外）` : ''}`
}
</script>

<template>
  <article class="card hidden-card">
    <h3 class="section-title">U 盘里的文件不见了</h3>
    <p class="muted small">
      插上 U 盘，文件夹不见了、变成了快捷方式，可空间还占着？多半是中了 U 盘病毒：它把原来的文件藏起来（设成「隐藏」和「系统」），再放一堆同名的快捷方式，一点就中毒。文件其实都还在，这里能把它们显示回来，改过的随时能改回去。
    </p>
    <p class="hint small">先别点 U 盘里的快捷方式，也别格式化 U 盘。</p>
    <p v-if="!isTauri()" class="muted small">浏览器里是演示：列出来的是一个中了病毒的 U 盘，不会改动真实的文件。</p>

    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="!!busy" @click="run('pick', hiddenPickFolder)">
        <BusySpinner v-if="busy === 'pick'" size="small" />{{ report ? '换一个 U 盘' : '选择 U 盘' }}
      </button>
      <button v-if="report" type="button" class="btn btn-ghost btn-small" :disabled="!!busy" @click="run('rescan', hiddenRescan)">
        <BusySpinner v-if="busy === 'rescan'" size="small" />重新查一遍
      </button>
      <span v-if="report" class="muted small path">{{ report.folder }}</span>
    </div>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <p v-if="done" class="success-text small" role="status">
      显示出来了 {{ done.changed }} 个文件和文件夹。<template v-if="done.keptPrograms">里面的 {{ done.keptPrograms }} 个程序和脚本文件多半是病毒，照样藏着。</template><template v-if="done.failed">有 {{ done.failed }} 个改不了（可能 U 盘被设成了只读，或者正被别的程序用着）。</template><template v-if="done.truncated">文件太多，没改完，再点一次「显示出来」接着改。</template>现在打开 U 盘看看，文件应该都回来了。
    </p>
    <p v-if="undone" class="small" role="status">
      改回去了 {{ undone.restored }} 处，又藏起来了。<template v-if="undone.failed">有 {{ undone.failed }} 处改不回去（文件可能被挪走、删掉了）。</template>
    </p>

    <template v-if="report">
      <p v-if="nothingHidden" class="muted small" role="status">
        这里没有被藏起来的文件。文件真的不见了的话，可能是被删掉了，或者 U 盘坏了：先别再往 U 盘里存东西，找懂哥用数据恢复软件试试。
      </p>
      <template v-if="report.items.length">
        <p class="small">被藏起来的文件和文件夹（{{ report.items.length }} 个），勾上的会显示出来：</p>
        <ul class="list">
          <li v-for="item in report.items" :key="item.id" class="entry">
            <label class="check">
              <input type="checkbox" :checked="selected.has(item.id)" :disabled="!!busy" @change="toggle(item.id)" />
              <span class="name">{{ item.name.trim() ? item.name : '（没有名字的文件夹）' }}</span>
            </label>
            <span class="muted small">{{ describe(item) }}</span>
          </li>
        </ul>
        <div class="row">
          <button type="button" class="btn btn-primary btn-small" :disabled="!!busy || !selectedCount" @click="restore">
            <BusySpinner v-if="busy === 'restore'" size="small" />显示出来（{{ selectedCount }} 个）
          </button>
        </div>
      </template>
      <div v-if="report.shortcuts.length" class="warn small">
        <p><strong>这些快捷方式是病毒放的，千万别双击：</strong>{{ report.shortcuts.join('、') }}</p>
        <p>文件都找回来以后，在 U 盘里选中它们，按 Delete 删掉。</p>
      </div>
      <div v-if="report.programs.length" class="warn small">
        <p><strong>这些被藏起来的程序和脚本多半就是病毒本身，小药箱不会把它们显示出来：</strong>{{ report.programs.join('、') }}</p>
        <p>用 Windows 安全中心查一下这个 U 盘（在「此电脑」里右键 U 盘，选「使用 Microsoft Defender 扫描」），这台电脑也全盘扫描一次；这个 U 盘插过的别的电脑，也要查一查。</p>
      </div>
      <div v-if="report.canUndo" class="row">
        <button type="button" class="btn btn-ghost btn-small" :disabled="!!busy" @click="undo">
          <BusySpinner v-if="busy === 'undo'" size="small" />改回去（重新藏起来，{{ report.canUndo }} 处）
        </button>
      </div>
    </template>
  </article>
</template>

<style scoped>
.hidden-card { display: flex; flex-direction: column; gap: 9px; grid-column: 1 / -1; }
.row { display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center; }
.path { overflow-wrap: anywhere; }
.hint { padding: 8px 11px; border-radius: var(--radius); background: var(--tone-info-bg); color: var(--tone-info-text); }
.warn { display: flex; flex-direction: column; gap: 4px; padding: 8px 11px; border-radius: var(--radius); background: var(--tone-advice-bg); }
.list { display: flex; flex-direction: column; gap: 6px; list-style: none; margin: 0; padding: 0; max-height: 300px; overflow: auto; }
.entry { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 14px; padding: 7px 11px; border-radius: var(--radius); background: var(--color-surface-2); }
.check { display: inline-flex; gap: 8px; align-items: center; min-width: 0; }
.name { font-weight: 600; overflow-wrap: anywhere; }
</style>
