<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { isTauri, renameApply, renamePreview, renameSelectFolder, renameUndo } from '../api'
import type { RenameEntry, RenameRules } from '../api/types'
import { errorText } from '../utils/format'

// 批量重命名：文件夹只能用系统的选择框选（界面传不了路径）；规则改了就要重新预览；改完能撤销上一次。
// 规则的顺序和后端一样：查找替换 → 换成「新名字 + 序号」→ 前面、后面加字；扩展名单独处理。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const folder = ref('')
const rules = reactive<RenameRules>({
  extensions: '',
  order: 'name',
  find: '',
  replace: '',
  numbering: false,
  base: '',
  start: 1,
  digits: 0,
  prefix: '',
  suffix: '',
  extension: 'keep',
  newExtension: '',
})
const entries = ref<RenameEntry[]>([])
const changed = ref(0)
const skipped = ref(0)
const busy = ref(false)
const error = ref('')
const message = ref('')
/** 上一次改名还能不能撤销 */
const canUndo = ref(false)

const unchanged = computed(() => entries.value.length - changed.value)

// 规则一改，之前的预览就不作数了
watch(rules, () => {
  entries.value = []
  error.value = ''
})

function clearResult(): void {
  entries.value = []
  error.value = ''
  message.value = ''
}

async function run(task: () => Promise<void>): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    await task()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = false
  }
}

function selectFolder(): Promise<void> {
  return run(async () => {
    const selected = await renameSelectFolder()
    if (selected) {
      folder.value = selected
      clearResult()
      canUndo.value = false
    }
  })
}

function preview(): Promise<void> {
  return run(async () => {
    message.value = ''
    entries.value = []
    const result = await renamePreview({ ...rules, start: Number(rules.start) || 0, digits: Number(rules.digits) || 0 })
    entries.value = result.entries
    changed.value = result.changed
    skipped.value = result.skipped
  })
}

function apply(): Promise<void> {
  return run(async () => {
    try {
      const count = await renameApply()
      message.value = `已经改好 ${count} 个文件的名字。改错了可以点「撤销这次改名」。`
      canUndo.value = true
    } finally {
      entries.value = []
    }
  })
}

function undo(): Promise<void> {
  return run(async () => {
    const count = await renameUndo()
    message.value = `已经把 ${count} 个文件改回原来的名字。`
    canUndo.value = false
  })
}
</script>

<template>
  <article class="card rename-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">批量重命名文件</h3>
    <p class="muted small">
      选一个文件夹，按下面的规则给里面的文件改名（不进子文件夹）。改之前先看预览，改完可以撤销；新名字和已有的文件重名、或者 Windows 不允许时，不会改。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里显示的是演示文件；在 Windows 的小药箱里处理真实文件。</p>
    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="selectFolder">选择文件夹</button>
      <span v-if="folder" class="muted small path">{{ folder }}</span>
    </div>

    <fieldset class="group" :disabled="!folder || busy">
      <label class="field-label" for="rename-ext">只改这些扩展名</label>
      <input id="rename-ext" v-model="rules.extensions" class="input" type="text" maxlength="120" placeholder="例如 jpg, png；不填就是全部" />

      <span class="field-label">序号按什么排</span>
      <div class="choices">
        <label><input v-model="rules.order" type="radio" value="name" /> 文件名</label>
        <label><input v-model="rules.order" type="radio" value="modified" /> 修改时间（照片按拍摄先后常用这个）</label>
      </div>

      <span class="field-label">查找替换（只改名字，不改扩展名；区分大小写）</span>
      <div class="pair">
        <input v-model="rules.find" class="input" type="text" maxlength="120" placeholder="查找，例如 IMG_" aria-label="查找" />
        <input v-model="rules.replace" class="input" type="text" maxlength="120" placeholder="替换成，可以不填" aria-label="替换成" />
      </div>

      <label class="check"><input v-model="rules.numbering" type="checkbox" /> 整个名字换成「新名字 + 序号」</label>
      <div v-if="rules.numbering" class="pair">
        <input v-model="rules.base" class="input" type="text" maxlength="120" placeholder="新名字，例如 旅行_" aria-label="新名字" />
        <label class="inline">从 <input v-model.number="rules.start" class="input small-input" type="number" min="0" max="99999999" aria-label="起始编号" /> 开始</label>
        <label class="inline">
          位数
          <select v-model.number="rules.digits" class="input small-input" aria-label="序号位数">
            <option :value="0">自动</option>
            <option :value="2">2 位（01）</option>
            <option :value="3">3 位（001）</option>
            <option :value="4">4 位（0001）</option>
          </select>
        </label>
      </div>

      <span class="field-label">前面、后面加字</span>
      <div class="pair">
        <input v-model="rules.prefix" class="input" type="text" maxlength="120" placeholder="前面加，例如 2026-" aria-label="前面加" />
        <input v-model="rules.suffix" class="input" type="text" maxlength="120" placeholder="后面加（在扩展名前面）" aria-label="后面加" />
      </div>

      <span class="field-label">扩展名</span>
      <div class="choices">
        <label><input v-model="rules.extension" type="radio" value="keep" /> 不变</label>
        <label><input v-model="rules.extension" type="radio" value="lower" /> 改成小写（.JPG → .jpg）</label>
        <label class="inline">
          <input v-model="rules.extension" type="radio" value="set" /> 改成
          <input v-model="rules.newExtension" class="input small-input" type="text" maxlength="20" placeholder="例如 txt" aria-label="新扩展名" :disabled="rules.extension !== 'set'" />
        </label>
      </div>
      <p v-if="rules.extension === 'set'" class="muted small">改扩展名不会转换文件的格式，只是换个名字；改错了打不开的话，撤销就行。</p>
    </fieldset>

    <button type="button" class="btn btn-secondary btn-small self-start" :disabled="busy || !folder" @click="preview">预览改名结果</button>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <p v-if="message" class="success-text small" role="status">{{ message }}</p>
    <button v-if="canUndo && !entries.length" type="button" class="btn btn-secondary btn-small self-start" :disabled="busy" @click="undo">撤销这次改名</button>

    <template v-if="entries.length">
      <p class="muted small" role="status">
        会改 {{ changed }} 个文件的名字<template v-if="unchanged">，{{ unchanged }} 个不变</template><template v-if="skipped">，{{ skipped }} 个扩展名不对、不处理</template>。核对以后再点「确认改名」。
      </p>
      <div class="preview-list">
        <table>
          <thead><tr><th scope="col">原来的名字</th><th scope="col">新名字</th></tr></thead>
          <tbody>
            <tr v-for="entry in entries" :key="entry.source" :class="{ same: !entry.changed }">
              <td>{{ entry.source }}</td>
              <td>{{ entry.changed ? entry.target : '（不变）' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <button type="button" class="btn btn-primary btn-small self-start" :disabled="busy || !changed" @click="apply">确认改名</button>
    </template>
  </article>
</template>

<style scoped>
.rename-card { display: flex; flex-direction: column; gap: 9px; }
.group { display: flex; flex-direction: column; gap: 7px; border: 0; padding: 0; margin: 0; min-width: 0; }
.field-label { font-weight: 600; font-size: var(--text-small); margin-top: 4px; }
.input { width: 100%; min-width: 0; padding: 7px 10px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); color: inherit; }
.small-input { width: 7.5em; display: inline-block; }
.row, .pair, .choices { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center; }
.pair > .input { flex: 1 1 12em; }
.inline, .check, .choices label { display: inline-flex; gap: 6px; align-items: center; font-size: var(--text-small); }
.self-start { align-self: flex-start; }
.path { overflow-wrap: anywhere; }
.preview-list { max-height: 260px; overflow: auto; border: 1px solid var(--color-border-strong); border-radius: var(--radius); }
table { width: 100%; border-collapse: collapse; text-align: left; font-size: var(--text-small); }
td, th { padding: 6px 9px; border-bottom: 1px solid var(--color-border-strong); overflow-wrap: anywhere; }
td { max-width: 180px; }
tr.same td { color: var(--color-text-muted, #777); }
</style>
