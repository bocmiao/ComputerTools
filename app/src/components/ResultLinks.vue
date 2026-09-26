<script setup lang="ts">
import { computed, onDeactivated } from 'vue'
import type { ToolSummary } from '../api/types'
import { toolLinkLabel } from '../labels'
import { findFeature, findSymptom, findTool, openSymptom, openTool } from '../state'
import { parseLink, type LinkKind, type ParsedLink } from '../utils/format'
import { useToolOpen } from '../utils/tools'
import AppIcon from './AppIcon.vue'
import BusySpinner from './BusySpinner.vue'

// 检测结果、小工具结果里 links 的按钮：
// - symptom:<id>  跳到「按症状修」的这个症状
// - feature:<id>  预览修复（由上层打开预览对话框）
// - tool:<id>     打开系统工具、「设置」页面的：直接打开，在下面说一句打开了没有；
//                 看信息、一键处理的：跳到「小工具」页并定位到这个小工具
// 目录里没有的小工具不显示按钮（例如数据比界面新）。
// 打开以后的那句话，换到别的页面再回来时就不显示了（和小工具页一样）。

const props = withDefaults(defineProps<{ links: string[]; kinds?: LinkKind[] }>(), {
  kinds: () => ['symptom', 'feature', 'tool'],
})
const emit = defineEmits<{ preview: [featureId: string] }>()

/** 认得的链接，去掉重复的 */
const parsed = computed(() => {
  const seen = new Set<string>()
  return props.links
    .map(parseLink)
    .filter((l): l is ParsedLink => l !== null && props.kinds.includes(l.kind))
    .filter((l) => {
      const key = `${l.kind}:${l.id}`
      if (seen.has(key)) return false
      seen.add(key)
      return true
    })
})
const symptomLinks = computed(() => parsed.value.filter((l) => l.kind === 'symptom'))
const featureLinks = computed(() => parsed.value.filter((l) => l.kind === 'feature'))
const tools = computed(() =>
  parsed.value
    .filter((l) => l.kind === 'tool')
    .map((l) => findTool(l.id))
    .filter((t): t is ToolSummary => t !== undefined),
)
const hasButtons = computed(() => symptomLinks.value.length + featureLinks.value.length + tools.value.length > 0)

function symptomTitle(id: string): string {
  return findSymptom(id)?.title ?? '相关症状'
}

function featureButtonText(id: string): string {
  if (featureLinks.value.length <= 1) return '预览修复'
  return `预览修复：${findFeature(id)?.title ?? id}`
}

// ── 打开系统工具 ──

const opener = useToolOpen()
// 「已经打开了」过一会儿就不是真的了：换到别的页面时清掉（页面在 KeepAlive 里，回来时卡片还在）
onDeactivated(opener.reset)
const openTools = computed(() => tools.value.filter((t) => t.group === 'open'))

/** 打开以后的那句话。同一张卡片里有好几个能打开的工具时，前面带上是哪一个 */
const openNotices = computed(() =>
  openTools.value.flatMap((t) => {
    const s = opener.states[t.id]
    if (!s || s.busy) return []
    const prefix = openTools.value.length > 1 ? `${t.title}：` : ''
    return [{ id: t.id, ok: s.ok === true, text: `${prefix}${s.text}` }]
  }),
)

function clickTool(t: ToolSummary): void {
  if (t.group === 'open') void opener.open(t)
  else openTool(t.id)
}
</script>

<template>
  <div v-if="hasButtons" class="result-links">
    <div class="actions">
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
      <button
        v-for="t in tools"
        :key="`t-${t.id}`"
        type="button"
        class="btn btn-secondary btn-small"
        @click="clickTool(t)"
      >
        <BusySpinner v-if="opener.states[t.id]?.busy" size="small" />
        <AppIcon v-else-if="t.group === 'open'" name="external" :size="16" />
        {{ toolLinkLabel(t) }}
      </button>
    </div>
    <p
      v-for="n in openNotices"
      :key="n.id"
      class="notice small"
      :class="n.ok ? 'success-text' : 'danger-text'"
      :role="n.ok ? 'status' : 'alert'"
    >
      <AppIcon v-if="n.ok" name="check" :size="14" class="notice-icon" />{{ n.text }}
    </p>
  </div>
</template>

<style scoped>
.result-links {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.notice {
  display: flex;
  align-items: flex-start;
  gap: 6px;
}

.notice-icon {
  margin-top: 3px;
}
</style>
