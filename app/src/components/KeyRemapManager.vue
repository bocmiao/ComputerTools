<script setup lang="ts">
import { computed, ref } from 'vue'
import { keyRemapGet, keyRemapSet } from '../api'
import type { KeyRemapView } from '../api/types'
import { openTest } from '../state'
import { errorText } from '../utils/format'
import BusySpinner from './BusySpinner.vue'

// 改键（键位重映射）：用 Windows 自带的扫描码映射（HKLM 的 Scancode Map），不装驱动、不在后台运行，对所有账户、
// 所有键盘都有效，重启电脑以后生效。整张表一起保存，每次保存都记进修改日志，能撤销。已经有别的软件设的、
// 小药箱认不出来的键时，只给「全部恢复」（整个清掉），不在这里接着改。点了才读。

/** 草稿里的一行；to 是空字符串：这个键不起作用 */
interface Row {
  uid: number
  from: string
  to: string
}

interface Preset {
  title: string
  rows: { from: string; to: string }[]
}

/** 最多改几个键，和引擎一样 */
const MAX_ROWS = 24

const PRESETS: Preset[] = [
  { title: '关掉 Win 键（打游戏不会跳回桌面）', rows: [{ from: 'MetaLeft', to: '' }, { from: 'MetaRight', to: '' }] },
  { title: '关掉 Caps Lock（不会误按成大写）', rows: [{ from: 'CapsLock', to: '' }] },
  { title: 'Caps Lock 改成左 Ctrl', rows: [{ from: 'CapsLock', to: 'ControlLeft' }] },
  { title: '关掉 Insert（打字不会吃掉后面的字）', rows: [{ from: 'Insert', to: '' }] },
  { title: '关掉 F1（误按不会打开帮助）', rows: [{ from: 'F1', to: '' }] },
]

const view = ref<KeyRemapView | null>(null)
const rows = ref<Row[]>([])
const loading = ref(false)
const saving = ref(false)
const error = ref<string | null>(null)
const notice = ref<string | null>(null)
/** 真的改过：提醒重启，给「测一测键盘」 */
const changed = ref(false)
let nextUid = 1

const labels = computed(() => new Map((view.value?.keys ?? []).map((k) => [k.id, k.label])))
/** 能改的键（多媒体键只能当「变成」的键） */
const sourceKeys = computed(() => (view.value?.keys ?? []).filter((k) => !k.targetOnly))
const mediaKeys = computed(() => (view.value?.keys ?? []).filter((k) => k.targetOnly))
const savedRows = computed(() => (view.value?.mappings ?? []).map((m) => ({ from: m.from, to: m.to ?? '' })))
const dirty = computed(
  () => JSON.stringify(rows.value.map(({ from, to }) => ({ from, to }))) !== JSON.stringify(savedRows.value),
)
/** 草稿里不能保存的地方：同一个键改了两次、改成它自己 */
const problem = computed<string | null>(() => {
  const seen = new Set<string>()
  for (const r of rows.value) {
    const name = labels.value.get(r.from) ?? r.from
    if (seen.has(r.from)) return `「${name}」改了两次，只能留一个。`
    seen.add(r.from)
    if (r.to === r.from) return `「${name}」改成它自己，等于没改。`
  }
  return null
})
const canClear = computed(() => !!view.value && (view.value.foreign || view.value.mappings.length > 0))

function resetRows(): void {
  rows.value = savedRows.value.map((r) => ({ uid: nextUid++, ...r }))
}

async function load(): Promise<void> {
  if (loading.value || saving.value) return
  loading.value = true
  error.value = null
  try {
    view.value = await keyRemapGet()
    resetRows()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

/** 常用的改法加进草稿；同一个键已经在改的，换成这一种 */
function addPreset(p: Preset): void {
  for (const r of p.rows) {
    const existing = rows.value.find((x) => x.from === r.from)
    if (existing) existing.to = r.to
    else if (rows.value.length < MAX_ROWS) rows.value.push({ uid: nextUid++, ...r })
  }
}

function addRow(): void {
  const free = sourceKeys.value.find((k) => !rows.value.some((r) => r.from === k.id))
  if (free && rows.value.length < MAX_ROWS) rows.value.push({ uid: nextUid++, from: free.id, to: '' })
}

function removeRow(uid: number): void {
  rows.value = rows.value.filter((r) => r.uid !== uid)
}

/** 别的行已经在改这个键了：下拉框里不给再选 */
function takenByOther(id: string, uid: number): boolean {
  return rows.value.some((r) => r.uid !== uid && r.from === id)
}

/** 保存草稿；clearAll：全部恢复（删掉整个改键设置） */
async function save(clearAll: boolean): Promise<void> {
  if (saving.value || loading.value) return
  saving.value = true
  error.value = null
  notice.value = null
  try {
    const mappings = clearAll ? [] : rows.value.map((r) => ({ from: r.from, to: r.to === '' ? null : r.to }))
    const r = await keyRemapSet(mappings)
    notice.value = r.message
    if (!r.ok) {
      error.value = r.error
      return
    }
    if (r.entryIds.length > 0) changed.value = true
    view.value = await keyRemapGet()
    resetRows()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <section class="group" aria-labelledby="settings-key-remap">
    <h2 id="settings-key-remap" class="section-title">改键（键位重映射）</h2>
    <div class="card menu-card">
      <p class="small">
        有的键老是误按（打游戏时按到 Win 键跳回桌面、Caps Lock 一碰就变成大写、按到 Insert 以后打字会吃掉后面的字），或者某个键坏了想用别的键代替，可以在这里改。用的是 Windows 自带的改键办法，不用装软件、也不在后台运行，对这台电脑的所有账户、所有键盘都有效，重启电脑以后生效。每一次保存都记在「修改日志」里，随时能恢复。
      </p>
      <p class="muted small">
        笔记本的 Fn 键和不少厂商自己的功能键（调亮度、开关触摸板这些）是键盘或者厂商的驱动自己处理的，不经过这里，改不了。关掉 Win 键以后，Win + D、Win + L 这些组合键也一起用不了。
      </p>
      <div class="row">
        <button type="button" class="btn btn-secondary btn-small" :disabled="loading || saving" @click="load">
          {{ view ? '重新读取' : '看看现在的改键' }}
        </button>
      </div>
      <p v-if="loading" class="loading-line" role="status"><BusySpinner size="small" />正在读取改键设置…</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
      <p v-if="notice" class="small" role="status">{{ notice }}</p>
      <div v-if="changed" class="row">
        <span class="small">重启以后，可以在键盘测试里按一按，看看改好了没有。</span>
        <button type="button" class="btn btn-secondary btn-small" @click="openTest('keyboard')">测一测键盘</button>
      </div>

      <template v-if="view && !loading">
        <template v-if="view.foreign">
          <div class="banner banner-warning" role="note">
            <div>
              <p class="banner-title">这台电脑已经有别的软件设的改键</p>
              <p class="small">
                里面有小药箱认不出来的：{{ view.foreignText }}。为了不弄乱，这里不能接着改，只能「全部恢复」：整个清掉，重启以后所有键恢复原样（在修改日志里能撤销）。
              </p>
            </div>
          </div>
          <ul v-if="view.mappings.length" class="list" aria-label="认得出来的改键">
            <li v-for="m in view.mappings" :key="m.from" class="entry small">{{ m.text }}</li>
          </ul>
        </template>

        <template v-else>
          <p class="small">常用的改法（点了加到下面，保存以后才生效）：</p>
          <div class="row">
            <button
              v-for="p in PRESETS"
              :key="p.title"
              type="button"
              class="btn btn-secondary btn-small"
              :disabled="saving"
              @click="addPreset(p)"
            >
              {{ p.title }}
            </button>
          </div>
          <p v-if="rows.length === 0" class="muted small">
            {{
              savedRows.length
                ? '改键都去掉了：保存以后重启电脑，所有键恢复原样。'
                : '现在没有改键，所有键都是原样。可以点上面常用的改法，或者「再加一个键」自己选。'
            }}
          </p>
          <ul v-else class="list" aria-label="改键">
            <li v-for="(r, i) in rows" :key="r.uid" class="entry">
              <div class="mapping">
                <span>按</span>
                <select
                  v-model="r.from"
                  class="input key-select"
                  :aria-label="`第 ${i + 1} 个：按下的键`"
                  :disabled="saving"
                >
                  <option v-for="k in sourceKeys" :key="k.id" :value="k.id" :disabled="takenByOther(k.id, r.uid)">
                    {{ k.label }}
                  </option>
                </select>
                <span>变成</span>
                <select v-model="r.to" class="input key-select" :aria-label="`第 ${i + 1} 个：变成什么`" :disabled="saving">
                  <option value="">不起作用（关掉这个键）</option>
                  <optgroup label="普通的键">
                    <option v-for="k in sourceKeys" :key="k.id" :value="k.id">{{ k.label }}</option>
                  </optgroup>
                  <optgroup label="多媒体键">
                    <option v-for="k in mediaKeys" :key="k.id" :value="k.id">{{ k.label }}</option>
                  </optgroup>
                </select>
              </div>
              <button type="button" class="btn btn-secondary btn-small" :disabled="saving" @click="removeRow(r.uid)">
                去掉
              </button>
            </li>
          </ul>
          <div class="row">
            <button
              type="button"
              class="btn btn-secondary btn-small"
              :disabled="saving || rows.length >= MAX_ROWS"
              @click="addRow"
            >
              再加一个键
            </button>
          </div>
          <p v-if="problem" class="danger-text small" role="alert">{{ problem }}</p>
        </template>

        <div class="row">
          <button
            v-if="!view.foreign"
            type="button"
            class="btn btn-primary"
            :disabled="!dirty || !!problem || saving"
            @click="save(false)"
          >
            <BusySpinner v-if="saving" size="small" />保存（重启以后生效）
          </button>
          <button v-if="!view.foreign && dirty" type="button" class="btn btn-secondary" :disabled="saving" @click="resetRows">
            放弃没保存的改动
          </button>
          <button v-if="canClear" type="button" class="btn btn-secondary" :disabled="saving" @click="save(true)">
            全部恢复
          </button>
        </div>
      </template>
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
.mapping { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; min-width: 0; }
.key-select { width: auto; min-width: 12em; max-width: 100%; }
</style>
