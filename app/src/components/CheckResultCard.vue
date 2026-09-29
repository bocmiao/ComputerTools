<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import type { CheckResult } from '../api/types'
import { fixerLabel, statusLabel, type ShownStatus } from '../labels'
import { simpleFacts } from '../utils/format'
import AppIcon from './AppIcon.vue'
import BusySpinner from './BusySpinner.vue'
import FactsTable from './FactsTable.vue'
import ResultLinks from './ResultLinks.vue'
import TagPill from './TagPill.vue'

// 体检结果的一行：状态点、名字、结论一句话；下面是跳转按钮（去修、预览修复、小工具）。
// 谁能修、下一步、详细数据、出错信息点「详情」才展开。
// selectable：这一项能放进「一键修好」，前面给勾选框；recheck：给「重查这一项」（没查出来的）。

const props = withDefaults(
  defineProps<{
    result: CheckResult & { status: ShownStatus }
    selectable?: boolean
    selected?: boolean
    recheck?: boolean
    rechecking?: boolean
    compact?: boolean
  }>(),
  { selectable: false, selected: false, recheck: false, rechecking: false, compact: false },
)
const emit = defineEmits<{ preview: [featureId: string]; toggle: [checked: boolean]; recheck: [] }>()

const expanded = ref(false)
const detailsId = useId()

const hasFacts = computed(() => simpleFacts(props.result.facts).length > 0)
const hasDetails = computed(() => !!(props.result.fixer || props.result.next || props.result.error || hasFacts.value))

function onToggle(e: Event): void {
  emit('toggle', (e.target as HTMLInputElement).checked)
}
</script>

<template>
  <li class="result" :class="{ compact }">
    <div class="line">
      <span v-if="!compact" class="check-slot">
        <input
          v-if="selectable"
          type="checkbox"
          :checked="selected"
          :aria-label="`一键修好时包括「${result.title}」`"
          @change="onToggle"
        />
      </span>
      <span class="dot" :class="`dot-${result.status}`" aria-hidden="true"></span>
      <div class="row-main">
        <h3 class="row-title title">
          <span>{{ result.title }}</span>
          <TagPill v-if="!compact" :tone="result.status">{{ statusLabel[result.status] }}</TagPill>
          <TagPill v-if="selectable" tone="info">能一键修</TagPill>
        </h3>
        <p class="message">{{ result.message }}</p>
      </div>
      <div class="row-actions">
        <button
          v-if="recheck"
          type="button"
          class="btn btn-secondary btn-small"
          :disabled="rechecking"
          @click="emit('recheck')"
        >
          <BusySpinner v-if="rechecking" size="small" />{{ rechecking ? '正在查…' : '重查这一项' }}
        </button>
        <button
          v-if="hasDetails"
          type="button"
          class="btn btn-ghost btn-small"
          :aria-expanded="expanded"
          :aria-controls="detailsId"
          @click="expanded = !expanded"
        >
          详情
          <AppIcon name="chevron-down" :size="16" class="chevron" :class="{ open: expanded }" />
        </button>
      </div>
    </div>

    <div v-if="result.links.length" class="links">
      <ResultLinks :links="result.links" @preview="(featureId) => emit('preview', featureId)" />
    </div>

    <div v-show="expanded" :id="detailsId" class="details">
      <dl class="kv">
        <template v-if="result.fixer">
          <dt>谁能修</dt>
          <dd>{{ fixerLabel[result.fixer] }}</dd>
        </template>
        <template v-if="result.next">
          <dt>下一步</dt>
          <dd>{{ result.next }}</dd>
        </template>
        <template v-if="result.error">
          <dt>出错信息</dt>
          <dd>{{ result.error }}</dd>
        </template>
      </dl>
      <FactsTable v-if="hasFacts" :facts="result.facts" />
    </div>
  </li>
</template>

<style scoped>
.result {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--color-divider);
}

.result:first-child {
  border-top: none;
}

.line {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.check-slot {
  display: flex;
  flex: none;
  justify-content: center;
  width: 18px;
  padding-top: 3px;
}

.check-slot input {
  width: 17px;
  height: 17px;
  margin: 0;
}

.dot {
  margin-top: 8px;
}

.title {
  font-size: var(--text-base);
}

.message {
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

.row-actions {
  flex-wrap: wrap;
  justify-content: flex-end;
}

/* 跳转按钮、打开以后的那句话：和上面的文字对齐 */
.links {
  padding-left: 52px;
}

.compact {
  gap: 4px;
  padding: 10px 16px;
}

.compact .links {
  padding-left: 22px;
}

.chevron {
  transition: transform 0.15s;
}

.chevron.open {
  transform: rotate(180deg);
}

.details {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-left: 52px;
  padding: 12px 14px;
  border-radius: var(--radius);
  background: var(--color-surface-2);
}

.compact .details {
  margin-left: 22px;
}
</style>
