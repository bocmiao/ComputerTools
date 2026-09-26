<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import type { CheckResult } from '../api/types'
import { fixerLabel, statusLabel, type ShownStatus } from '../labels'
import { findFeature, findSymptom, openSymptom } from '../state'
import { parseLink, simpleFacts } from '../utils/format'
import AppIcon from './AppIcon.vue'
import FactsTable from './FactsTable.vue'
import TagPill from './TagPill.vue'

// 体检结果的一张卡片。结论和说明直接显示；谁能修、下一步、详细数据、出错信息展开后才显示。

const props = defineProps<{ result: CheckResult & { status: ShownStatus } }>()
const emit = defineEmits<{ preview: [featureId: string] }>()

const expanded = ref(false)
const detailsId = useId()

const links = computed(() => props.result.links.map(parseLink).filter((l) => l !== null))
const symptomLinks = computed(() => links.value.filter((l) => l.kind === 'symptom'))
const featureLinks = computed(() => links.value.filter((l) => l.kind === 'feature'))
const hasFacts = computed(() => simpleFacts(props.result.facts).length > 0)
const hasDetails = computed(() => !!(props.result.fixer || props.result.next || props.result.error || hasFacts.value))

function symptomTitle(id: string): string {
  return findSymptom(id)?.title ?? '相关症状'
}

function featureButtonText(id: string): string {
  if (featureLinks.value.length <= 1) return '预览修复'
  return `预览修复：${findFeature(id)?.title ?? id}`
}
</script>

<template>
  <article class="card result-card" :class="[`status-${result.status}`, { compact: result.status === 'ok' }]">
    <div class="head">
      <TagPill :tone="result.status" dot>{{ statusLabel[result.status] }}</TagPill>
      <h3 class="title">{{ result.title }}</h3>
      <button
        v-if="hasDetails"
        type="button"
        class="btn btn-ghost btn-small toggle"
        :aria-expanded="expanded"
        :aria-controls="detailsId"
        @click="expanded = !expanded"
      >
        {{ expanded ? '收起详情' : '展开详情' }}
        <AppIcon name="chevron-down" :size="16" class="chevron" :class="{ open: expanded }" />
      </button>
    </div>
    <p class="message">{{ result.message }}</p>

    <div v-if="links.length" class="actions">
      <button
        v-for="link in symptomLinks"
        :key="`s-${link.id}`"
        type="button"
        class="btn btn-secondary btn-small"
        @click="openSymptom(link.id)"
      >
        去修：{{ symptomTitle(link.id) }}
      </button>
      <button
        v-for="link in featureLinks"
        :key="`f-${link.id}`"
        type="button"
        class="btn btn-primary btn-small"
        :title="findFeature(link.id)?.title"
        @click="emit('preview', link.id)"
      >
        {{ featureButtonText(link.id) }}
      </button>
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
  </article>
</template>

<style scoped>
.result-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  border-left-width: 4px;
}

/* 正常的项目排在后面，做得紧凑一些 */
.result-card.compact {
  gap: 4px;
  padding-top: 12px;
  padding-bottom: 12px;
}

.compact .title {
  font-size: var(--text-base);
}

.status-ok {
  border-left-color: var(--tone-ok-dot);
}

.status-advice {
  border-left-color: var(--tone-advice-dot);
}

.status-manual {
  border-left-color: var(--tone-manual-dot);
}

.status-unknown {
  border-left-color: var(--tone-unknown-dot);
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}

.title {
  font-size: var(--text-large);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 4px;
}

.toggle {
  margin-left: auto;
  margin-right: -8px;
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
  padding-top: 10px;
  margin-top: 2px;
  border-top: 1px dashed var(--color-border);
}
</style>
