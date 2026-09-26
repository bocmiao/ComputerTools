<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { featureApply } from '../api'
import type { ApplyResult, FeatureSummary, Reboot } from '../api/types'
import { applyOutcome, applyOutcomeTitle, rebootLabel, rebootWeight, type ApplyOutcome } from '../labels'
import { errorText } from '../utils/format'
import { vAutofocus } from '../utils/dialogs'
import AppIcon from './AppIcon.vue'
import BusySpinner from './BusySpinner.vue'
import ExplorerRestart from './ExplorerRestart.vue'
import ModalDialog from './ModalDialog.vue'
import StatusLamp, { type LampState } from './StatusLamp.vue'

// 「只应用推荐项」：一个一个预览太繁琐，这里列出汇总，确认后依次执行。
// 每一项按四种结果显示（已经改好 / 改了但没确认生效 / 不用改 / 没改成）；一项没改成不影响后面的项。

const props = defineProps<{ features: FeatureSummary[] }>()
/** finished 的参数：这次有没有真的改过东西（改过的话，体检结果可能已经过期） */
const emit = defineEmits<{ close: []; finished: [changed: boolean] }>()

type ItemStatus = 'waiting' | 'running' | ApplyOutcome
interface Item {
  feature: FeatureSummary
  status: ItemStatus
  result: ApplyResult | null
  /** 执行命令本身出错、没拿到结果时的原因 */
  error: string | null
}

const items = reactive<Item[]>(props.features.map((feature) => ({ feature, status: 'waiting', result: null, error: null })))
const phase = ref<'confirm' | 'applying' | 'done'>('confirm')
const currentIndex = ref(0)

const withRestorePoint = computed(() => items.filter((i) => i.feature.risk !== 'safe').length)
const irreversible = computed(() => items.filter((i) => !i.feature.reversible))

const counts = computed(() => {
  const c: Record<ApplyOutcome, number> = { done: 0, unverified: 0, unchanged: 0, failed: 0 }
  for (const i of items) if (i.status !== 'waiting' && i.status !== 'running') c[i.status]++
  return c
})

/** 汇总的颜色：有没改成的算红，有没确认生效的算橙，否则绿 */
const summaryTone = computed<ApplyOutcome>(() =>
  counts.value.failed ? 'failed' : counts.value.unverified ? 'unverified' : 'done',
)

const summaryTitle = computed(() => {
  const c = counts.value
  if (!c.failed && !c.unverified) return '推荐的设置都应用好了'
  const parts: string[] = []
  if (c.done + c.unchanged) parts.push(`${c.done + c.unchanged} 项已经好了`)
  if (c.unverified) parts.push(`${c.unverified} 项改了但还没确认生效`)
  if (c.failed) parts.push(`${c.failed} 项没有改成`)
  return parts.join('，')
})

/** 真的改过东西的项目里最「重」的重启要求（没改成的、本来就好的，后端给的都是 none） */
const reboot = computed<Reboot>(() => {
  let strongest: Reboot = 'none'
  for (const i of items) {
    const r = i.result?.ok ? i.result.reboot : 'none'
    if (rebootWeight[r] > rebootWeight[strongest]) strongest = r
  }
  return strongest
})

const lampState: Record<ItemStatus, LampState> = {
  waiting: 'pending',
  running: 'running',
  done: 'ok',
  unchanged: 'ok',
  unverified: 'advice',
  failed: 'manual',
}

function lampLabel(i: Item): string {
  if (i.status === 'waiting') return '等待应用'
  if (i.status === 'running') return '正在应用'
  return applyOutcomeTitle[i.status]
}

async function start(): Promise<void> {
  if (phase.value !== 'confirm') return
  phase.value = 'applying'
  for (const [index, item] of items.entries()) {
    currentIndex.value = index
    item.status = 'running'
    try {
      const r = await featureApply(item.feature.id)
      item.result = r
      item.status = applyOutcome(r)
    } catch (e) {
      item.status = 'failed'
      item.error = errorText(e)
    }
  }
  phase.value = 'done'
}

function close(): void {
  if (phase.value === 'done') emit('finished', items.some((i) => (i.result?.entryIds.length ?? 0) > 0))
  emit('close')
}
</script>

<template>
  <ModalDialog title="只应用推荐项" :busy="phase === 'applying'" wide @close="close">
    <template v-if="phase === 'confirm'">
      <p>下面这 {{ items.length }} 项推荐设置还没有设置好，确认后会一项一项地应用：</p>
    </template>
    <p v-else-if="phase === 'applying'" class="loading-line" role="status">
      <BusySpinner size="small" />正在应用第 {{ currentIndex + 1 }} 项（共 {{ items.length }} 项）…
    </p>
    <div v-else class="result" :class="`result-${summaryTone}`" role="status">
      <p class="result-title">{{ summaryTitle }}</p>
      <p v-if="counts.unchanged">其中 {{ counts.unchanged }} 项本来就是好的，没有改动。</p>
      <p v-if="reboot !== 'none'">
        <strong>{{ rebootLabel[reboot] }}</strong>，之后才能看到全部效果。
      </p>
      <!-- 最重的要求只是重启资源管理器时，直接给按钮（要注销、重启的话，那时资源管理器会跟着重开） -->
      <ExplorerRestart v-if="reboot === 'explorer'" />
    </div>

    <ol class="items">
      <li v-for="item in items" :key="item.feature.id" class="item">
        <StatusLamp :state="lampState[item.status]" :label="lampLabel(item)" />
        <div class="item-body">
          <p class="item-title">{{ item.feature.title }}</p>
          <p class="muted small">{{ item.feature.description }}</p>
          <p v-if="item.feature.reboot !== 'none' && phase === 'confirm'" class="muted small">
            改完{{ rebootLabel[item.feature.reboot] }}
          </p>
          <p v-if="item.status === 'done' || item.status === 'unchanged'" class="small success-text">
            <AppIcon name="check" :size="14" /> {{ item.result?.message || applyOutcomeTitle[item.status] }}
          </p>
          <p v-else-if="item.status === 'unverified'" class="small warn-text">
            {{ applyOutcomeTitle.unverified }}。{{ item.result?.message }}
          </p>
          <template v-else-if="item.status === 'failed'">
            <p class="small danger-text">{{ item.result?.message || `没有改成：${item.error ?? '原因不明'}` }}</p>
            <p v-if="item.result?.error" class="small muted">出错信息：{{ item.result.error }}</p>
          </template>
          <ul v-if="item.result && item.result.notes.length" class="notes small muted">
            <li v-for="(n, i) in item.result.notes" :key="i">{{ n }}</li>
          </ul>
        </div>
      </li>
    </ol>

    <template v-if="phase === 'confirm'">
      <p class="muted">每一项都会记进「修改日志」，随时可以恢复原状。</p>
      <p v-if="withRestorePoint" class="muted">其中 {{ withRestorePoint }} 项执行前会先创建系统还原点。</p>
      <p v-for="item in irreversible" :key="item.feature.id" class="danger-text">
        「{{ item.feature.title }}」不能撤销：{{ item.feature.irreversibleReason ?? '这一项改了就退不回去。' }}
      </p>
    </template>

    <template #footer>
      <template v-if="phase !== 'done'">
        <button type="button" class="btn btn-secondary" :disabled="phase === 'applying'" @click="close">取消</button>
        <button type="button" class="btn btn-primary" :disabled="phase === 'applying'" @click="start">
          确认应用 {{ items.length }} 项
        </button>
      </template>
      <button v-else type="button" class="btn btn-primary" v-autofocus @click="close">完成</button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.items {
  display: flex;
  flex-direction: column;
  list-style: none;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
}

.item {
  display: flex;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--color-border);
}

.item:last-child {
  border-bottom: none;
}

.item-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.item-title {
  font-weight: 600;
}

.notes {
  padding-left: 1.3em;
}

.result {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 14px 18px;
  border-radius: var(--radius);
}

.result-done {
  background: var(--tone-ok-bg);
  color: var(--tone-ok-text);
}

.result-unverified {
  background: var(--tone-advice-bg);
  color: var(--tone-advice-text);
}

.result-failed {
  background: var(--tone-manual-bg);
  color: var(--tone-manual-text);
}

.warn-text {
  color: var(--tone-advice-text);
}

.result-title {
  font-size: var(--text-large);
  font-weight: 600;
}
</style>
