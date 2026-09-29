<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { diffTexts, withContext, type DiffLine, type ShownLine, type TextDiffResult } from '../utils/textDiff'

// 文本对比：合同、通知、文章改过以后，看看到底改了哪里。逐行比较，改过的行再逐字标出不一样的字（utils/textDiff.ts）。
// 只在本机计算；默认只显示改动和前后两行，中间相同的折起来。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const before = ref('')
const after = ref('')
const ignoreSpaces = ref(false)
const onlyChanges = ref(true)
const result = ref<TextDiffResult | null>(null)

watch([before, after, ignoreSpaces], () => {
  result.value = null
})

function compare(): void {
  result.value = diffTexts(before.value, after.value, { ignoreSpaces: ignoreSpaces.value })
}

const shown = computed<ShownLine[]>(() => {
  if (!result.value) return []
  return onlyChanges.value ? withContext(result.value.lines, 2) : result.value.lines
})

const summary = computed(() => {
  const r = result.value
  if (!r) return ''
  if (r.same) return ignoreSpaces.value ? '两段文字一样（不算每行首尾的空格）。' : '两段文字一模一样。'
  const details: string[] = []
  if (r.removed) details.push(`原来的 ${r.removed} 行删掉或者改了`)
  if (r.added) details.push(`新加和改过的有 ${r.added} 行`)
  return `有 ${r.blocks} 处不一样：${details.join('，')}。`
})

function marker(line: DiffLine): string {
  return line.op === 'add' ? '+' : line.op === 'del' ? '−' : ''
}

function swap(): void {
  ;[before.value, after.value] = [after.value, before.value]
}
</script>

<template>
  <article class="card diff-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">文本对比</h3>
    <p class="muted small">
      合同、通知、文章改过以后，把原来的和改过的分别粘进来，看看到底改了哪里：删掉的行标红，多出来的行标绿，改过的行里不一样的字加粗。只在这台电脑里比较，不上传。
    </p>
    <div class="diff-inputs">
      <div class="diff-input">
        <label class="field-label" for="diff-before">原来的</label>
        <textarea id="diff-before" v-model="before" class="input text-box" rows="7" spellcheck="false"></textarea>
      </div>
      <div class="diff-input">
        <label class="field-label" for="diff-after">改过的</label>
        <textarea id="diff-after" v-model="after" class="input text-box" rows="7" spellcheck="false"></textarea>
      </div>
    </div>
    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="!before && !after" @click="compare">对比</button>
      <button type="button" class="btn btn-ghost btn-small" :disabled="!before && !after" @click="swap">左右互换</button>
      <label class="check small"><input v-model="ignoreSpaces" type="checkbox" /> 不管每行首尾的空格</label>
    </div>
    <template v-if="result">
      <p class="small" :class="result.same ? 'success-text' : ''" role="status">{{ summary }}</p>
      <p v-if="result.tooDifferent" class="muted small">两段文字差别太大，没有逐行细比，整段算作换掉了。</p>
      <template v-if="!result.same">
        <label class="check small"><input v-model="onlyChanges" type="checkbox" /> 只看改动的地方（前后各留两行）</label>
        <ol class="diff-lines" aria-label="对比结果">
          <template v-for="(line, i) in shown" :key="i">
            <li v-if="line.op === 'gap'" class="diff-gap muted small">…… 中间 {{ line.count }} 行一样 ……</li>
            <li v-else class="diff-line" :class="`diff-${line.op}`">
              <span class="diff-no" aria-hidden="true">{{ line.op === 'add' ? '' : line.oldNo }}</span>
              <span class="diff-no" aria-hidden="true">{{ line.op === 'del' ? '' : line.newNo }}</span>
              <span class="diff-mark" :aria-label="line.op === 'add' ? '多出来的' : line.op === 'del' ? '删掉的' : undefined">{{ marker(line) }}</span>
              <span class="diff-text">
                <template v-if="line.pieces">
                  <span v-for="(p, j) in line.pieces" :key="j" :class="{ 'diff-changed': p.changed }">{{ p.text }}</span>
                </template>
                <template v-else>{{ line.text || ' ' }}</template>
              </span>
            </li>
          </template>
        </ol>
      </template>
    </template>
  </article>
</template>

<style scoped>
.diff-card { display: flex; flex-direction: column; gap: 9px; grid-column: 1 / -1; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); color: inherit; }
.text-box { resize: vertical; line-height: 1.5; }
.diff-inputs { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
.diff-input { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.row { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
.check { display: inline-flex; align-items: center; gap: 6px; }
.diff-lines {
  list-style: none; max-height: 420px; overflow: auto; border: 1px solid var(--color-border); border-radius: var(--radius);
  font-size: var(--text-small); line-height: 1.55;
}
.diff-line { display: grid; grid-template-columns: 3.2em 3.2em 1.4em minmax(0, 1fr); padding: 1px 8px 1px 0; }
.diff-no { color: var(--color-text-muted); text-align: right; padding-right: 6px; font-variant-numeric: tabular-nums; user-select: none; }
.diff-mark { font-weight: 700; user-select: none; }
.diff-text { white-space: pre-wrap; overflow-wrap: anywhere; }
.diff-add { background: var(--tone-ok-bg); color: var(--tone-ok-text); }
.diff-del { background: var(--tone-manual-bg); color: var(--tone-manual-text); }
.diff-add .diff-changed { background: color-mix(in srgb, var(--tone-ok-dot) 30%, transparent); font-weight: 700; border-radius: 3px; }
.diff-del .diff-changed { background: color-mix(in srgb, var(--tone-manual-dot) 30%, transparent); font-weight: 700; border-radius: 3px; text-decoration: line-through; }
.diff-gap { padding: 3px 12px; background: var(--color-surface-2); }
@media (max-width: 760px) {
  .diff-inputs { grid-template-columns: minmax(0, 1fr); }
}
</style>
