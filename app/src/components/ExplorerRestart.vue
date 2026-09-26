<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef } from 'vue'
import { toolRun } from '../api'
import { findTool, RESTART_EXPLORER_TOOL } from '../state'
import { errorText, parseLink } from '../utils/format'
import AppIcon from './AppIcon.vue'
import BusySpinner from './BusySpinner.vue'
import ConfirmDialog from './ConfirmDialog.vue'
import ResultLinks from './ResultLinks.vue'

// 改完「需要重启资源管理器」的设置以后，在提示旁边直接给「现在重启资源管理器」。
// 用的是小工具「重启资源管理器」：目录里没有这个小工具就什么都不显示；它要求确认的话先问一下（说法取自目录）。
// 结果整个显示出来：没重新打开时（桌面和任务栏可能已经不见了），「下一步」教用户从任务管理器里打开，
// 结果里的「打开任务管理器」按钮也要在。这个按钮常在对话框里，所以只给「打开系统工具、设置页面」的按钮：
// 去修、预览修复、跳到小工具页都会离开当前页面，对话框却还开着。

const tool = computed(() => {
  const t = findTool(RESTART_EXPLORER_TOOL)
  return t?.group === 'action' ? t : null
})

const confirming = ref(false)
const running = ref(false)
interface Outcome {
  ok: boolean
  text: string
  next: string | null
  links: string[]
  error: string | null
}

const outcome = ref<Outcome | null>(null)

/** 结果里能直接打开的系统工具（见上面的说明） */
const openLinks = computed(() =>
  (outcome.value?.links ?? []).filter((l) => {
    const p = parseLink(l)
    return p?.kind === 'tool' && findTool(p.id)?.group === 'open'
  }),
)
const message = useTemplateRef<HTMLElement>('message')

function start(): void {
  if (!tool.value || running.value) return
  if (tool.value.confirm) confirming.value = true
  else void run()
}

async function run(): Promise<void> {
  if (running.value) return
  running.value = true
  outcome.value = null
  let next: Outcome
  try {
    const r = await toolRun(RESTART_EXPLORER_TOOL)
    next = { ok: r.status === 'ok', text: r.message, next: r.next, links: r.links, error: r.error }
  } catch (e) {
    next = { ok: false, text: `没能重启资源管理器：${errorText(e)}`, next: null, links: [], error: null }
  }
  running.value = false
  // 先关确认框（焦点回到按钮上），再显示结果；成功以后按钮就不要了，焦点交给结果那句话
  confirming.value = false
  await nextTick()
  outcome.value = next
  await nextTick()
  // 没有确认框时按钮在执行期间被禁用过，焦点可能已经掉到页面上了
  const lost = !document.activeElement || document.activeElement === document.body
  if (next.ok || lost) message.value?.focus()
}
</script>

<template>
  <div v-if="tool" class="explorer-restart">
    <div v-if="!outcome?.ok">
      <button type="button" class="btn btn-secondary btn-small" :disabled="running" @click="start">
        <BusySpinner v-if="running" size="small" />
        {{ running ? '正在重启资源管理器…' : '现在重启资源管理器' }}
      </button>
    </div>
    <template v-if="outcome">
      <div class="outcome-text" :role="outcome.ok ? 'status' : 'alert'">
        <p
          ref="message"
          class="small outcome"
          :class="outcome.ok ? 'success-text' : 'danger-text'"
          tabindex="-1"
        >
          <AppIcon v-if="outcome.ok" name="check" :size="14" class="outcome-icon" />{{ outcome.text }}
        </p>
        <p v-if="outcome.next" class="small">下一步：{{ outcome.next }}</p>
        <p v-if="outcome.error" class="small muted">出错信息：{{ outcome.error }}</p>
      </div>
      <ResultLinks :links="openLinks" :kinds="['tool']" />
    </template>

    <ConfirmDialog
      v-if="confirming"
      :title="`${tool.title}？`"
      :confirm-text="tool.title"
      :busy="running"
      @confirm="run"
      @close="confirming = false"
    >
      <p>{{ tool.confirm }}</p>
    </ConfirmDialog>
  </div>
</template>

<style scoped>
.explorer-restart {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}

.outcome-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-wrap: anywhere;
}

.outcome {
  display: flex;
  align-items: flex-start;
  gap: 6px;
}

.outcome:focus {
  outline: none;
}

.outcome-icon {
  margin-top: 3px;
}
</style>
