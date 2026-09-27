<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onDeactivated, reactive, ref, useId, useTemplateRef, watch } from 'vue'
import type { ToolResult } from '../api/types'
import AppIcon from './AppIcon.vue'
import ToolOutcome from './ToolOutcome.vue'

// 「看信息」小工具的结果：结论、几张「标签：值」的小表、「复制全部」。
//
// secret 的值（例如 WiFi 密码）默认遮住：点「显示」才放进页面，「复制全部」不带它们，
// 要的话单独点那一行的「复制」。重新查看、离开这个页面时，全部重新遮住。
// 这些值不写进控制台，也不放进 title、aria-label 这类属性。

const props = defineProps<{ result: ToolResult; running: boolean }>()
const emit = defineEmits<{ preview: [featureId: string] }>()

/** 遮住时显示的点：个数固定，不透露密码有多长 */
const MASK = '••••••••'
const COPIED_MS = 4000

const uid = useId()
const revealed = reactive(new Set<string>())
/** 单独复制一行的结果 */
const rowCopy = reactive(new Map<string, 'ok' | 'fail'>())
const copyAllState = ref<'idle' | 'ok' | 'fail'>('idle')
const fallback = useTemplateRef<HTMLTextAreaElement>('fallback')
let rowCopyTimer: ReturnType<typeof setTimeout> | undefined
let copyAllTimer: ReturnType<typeof setTimeout> | undefined

function rowKey(i: number, j: number): string {
  return `${i}-${j}`
}

function valueId(key: string): string {
  return `${uid}-value-${key}`
}

/** 模板里用：每张表、每一行，带上遮住与否用的 key */
const tables = computed(() =>
  props.result.sections.map((s, i) => ({
    key: i,
    title: s.title,
    rows: s.rows.map((r, j) => ({ ...r, key: rowKey(i, j) })),
  })),
)
const hasRows = computed(() => props.result.sections.some((s) => s.rows.length > 0))
const hasSecrets = computed(() => props.result.sections.some((s) => s.rows.some((r) => r.secret)))

/** 「复制全部」的纯文本：表格标题和「标签：值」，secret 的行一律不带 */
const copyText = computed(() => {
  const lines = [props.result.title]
  for (const s of props.result.sections) {
    lines.push('', `【${s.title}】`)
    for (const r of s.rows) if (!r.secret) lines.push(`${r.label}：${r.value}`)
  }
  return lines.join('\n')
})

function maskAll(): void {
  revealed.clear()
  rowCopy.clear()
  copyAllState.value = 'idle'
  clearTimeout(rowCopyTimer)
  clearTimeout(copyAllTimer)
}

// 重新查看（开始时就遮住，不等新结果回来）、换了结果、离开页面：全部重新遮住
watch(
  () => props.running,
  (running) => {
    if (running) maskAll()
  },
)
watch(() => props.result, maskAll)
onDeactivated(maskAll)
onBeforeUnmount(maskAll)

function toggle(key: string): void {
  if (revealed.has(key)) revealed.delete(key)
  else revealed.add(key)
}

async function writeClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return false
  }
}

async function copyRow(key: string, value: string): Promise<void> {
  clearTimeout(rowCopyTimer)
  rowCopy.clear()
  if (await writeClipboard(value)) {
    rowCopy.set(key, 'ok')
    rowCopyTimer = setTimeout(() => rowCopy.delete(key), COPIED_MS)
    return
  }
  // 自动复制不行：把这一行显示出来并选中，方便按 Ctrl+C
  rowCopy.set(key, 'fail')
  revealed.add(key)
  await nextTick()
  const el = document.getElementById(valueId(key))
  if (el) window.getSelection()?.selectAllChildren(el)
}

async function copyAll(): Promise<void> {
  clearTimeout(copyAllTimer)
  if (await writeClipboard(copyText.value)) {
    copyAllState.value = 'ok'
    copyAllTimer = setTimeout(() => (copyAllState.value = 'idle'), COPIED_MS)
    return
  }
  // 自动复制不行：把要复制的文字放进文本框里选中（和诊断报告页一样）
  copyAllState.value = 'fail'
  await nextTick()
  fallback.value?.focus()
  fallback.value?.select()
}
</script>

<template>
  <div class="info-result" :class="{ stale: running }" :aria-busy="running">
    <ToolOutcome :result="result" @preview="(featureId) => emit('preview', featureId)" />

    <template v-if="hasRows">
      <div class="copy-bar">
        <button type="button" class="btn btn-secondary btn-small" @click="copyAll">
          <AppIcon name="copy" :size="16" />复制全部
        </button>
        <p v-if="hasSecrets" class="small muted">遮住的内容（例如密码）不会一起复制，需要的话点那一行的「复制」。</p>
      </div>
      <p v-if="copyAllState === 'ok'" class="small success-text" role="status">
        已经复制好了，可以粘贴到微信里发给别人。
      </p>
      <template v-else-if="copyAllState === 'fail'">
        <p class="small danger-text" role="alert">没能自动复制。下面的文字已经选中了，请按 Ctrl+C 复制。</p>
        <label :for="`${uid}-fallback`" class="visually-hidden">要复制的文字</label>
        <textarea
          :id="`${uid}-fallback`"
          ref="fallback"
          class="input fallback"
          :value="copyText"
          readonly
          rows="6"
          spellcheck="false"
        ></textarea>
      </template>

      <div class="sections">
        <div v-for="t in tables" :key="t.key" class="table-wrap">
          <table class="table">
            <caption class="section-caption">{{ t.title }}</caption>
            <tbody>
              <tr v-for="r in t.rows" :key="r.key" :class="{ 'secret-row': r.secret }">
                <th scope="row" class="row-label">{{ r.label }}</th>
                <td v-if="r.secret">
                  <div class="secret-cell">
                    <div class="secret">
                      <span v-if="revealed.has(r.key)" :id="valueId(r.key)" class="mono secret-value">{{ r.value }}</span>
                      <span v-else class="secret-mask">
                        <span aria-hidden="true">{{ MASK }}</span>
                        <span class="visually-hidden">已遮住</span>
                      </span>
                      <span class="secret-actions">
                        <button
                          type="button"
                          class="btn btn-secondary btn-small"
                          :aria-label="`${revealed.has(r.key) ? '隐藏' : '显示'}${r.label}`"
                          @click="toggle(r.key)"
                        >
                          {{ revealed.has(r.key) ? '隐藏' : '显示' }}
                        </button>
                        <button
                          type="button"
                          class="btn btn-secondary btn-small"
                          :aria-label="`复制${r.label}`"
                          @click="copyRow(r.key, r.value)"
                        >
                          复制
                        </button>
                      </span>
                    </div>
                    <p v-if="rowCopy.get(r.key) === 'ok'" class="small success-text" role="status">已复制{{ r.label }}。</p>
                    <p v-else-if="rowCopy.get(r.key) === 'fail'" class="small danger-text" role="alert">
                      没能自动复制，已经显示出来并选中了，请按 Ctrl+C 复制。
                    </p>
                  </div>
                </td>
                <td v-else class="row-value">{{ r.value }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.info-result {
  display: flex;
  flex-direction: column;
  gap: 10px;
  transition: opacity 0.15s;
}

/* 重新查看时，旧结果淡一点 */
.info-result.stale {
  opacity: 0.55;
}

.copy-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
}

.fallback {
  font-family: var(--font-mono);
  font-size: var(--text-small);
  resize: vertical;
}

.sections {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 10px;
  align-items: start;
}

.section-caption {
  padding: 8px 12px;
  background: var(--color-surface-2);
  border-bottom: 1px solid var(--color-border);
  font-size: var(--text-base);
  font-weight: 600;
  text-align: left;
  overflow-wrap: anywhere;
}

.row-label {
  width: 1%;
  white-space: nowrap;
  font-weight: 400;
  color: var(--color-text-muted);
}

/* 遮住的那一行有按钮，比别的行高：标签和圆点、按钮对齐 */
.secret-row > .row-label {
  vertical-align: middle;
}

.row-value {
  overflow-wrap: anywhere;
}

.secret-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.secret {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 10px;
}

.secret-value {
  font-size: var(--text-base);
  /* BitLocker 恢复密钥有 55 个字符，窄屏上要能折行 */
  overflow-wrap: anywhere;
}

.secret-mask {
  letter-spacing: 0.1em;
  color: var(--color-text-muted);
}

.secret-actions {
  display: inline-flex;
  gap: 6px;
}
</style>
