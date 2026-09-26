<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { featureApply } from '../api'
import type { ApplyResult, FeatureSummary, Reboot } from '../api/types'
import { rebootLabel, rebootWeight } from '../labels'
import { errorText } from '../utils/format'
import { vAutofocus } from '../utils/dialogs'
import AppIcon from './AppIcon.vue'
import BusySpinner from './BusySpinner.vue'
import ModalDialog from './ModalDialog.vue'
import StatusLamp from './StatusLamp.vue'

// 「只应用推荐项」：一个一个预览太繁琐，这里列出汇总，确认后依次执行。

const props = defineProps<{ features: FeatureSummary[] }>()
const emit = defineEmits<{ close: []; finished: [] }>()

type ItemStatus = 'waiting' | 'running' | 'ok' | 'failed'
interface Item {
  feature: FeatureSummary
  status: ItemStatus
  result: ApplyResult | null
  error: string | null
}

const items = reactive<Item[]>(props.features.map((feature) => ({ feature, status: 'waiting', result: null, error: null })))
const phase = ref<'confirm' | 'applying' | 'done'>('confirm')
const currentIndex = ref(0)

const withRestorePoint = computed(() => items.filter((i) => i.feature.risk !== 'safe').length)
const irreversible = computed(() => items.filter((i) => !i.feature.reversible))
const okCount = computed(() => items.filter((i) => i.status === 'ok').length)
const failedCount = computed(() => items.filter((i) => i.status === 'failed').length)

/** 所有成功的项目里最「重」的重启要求 */
const reboot = computed<Reboot>(() => {
  let strongest: Reboot = 'none'
  for (const i of items) {
    const r = i.result?.ok ? i.result.reboot : 'none'
    if (rebootWeight[r] > rebootWeight[strongest]) strongest = r
  }
  return strongest
})

function lamp(i: Item) {
  switch (i.status) {
    case 'running':
      return 'running' as const
    case 'ok':
      return 'ok' as const
    case 'failed':
      return 'advice' as const
    default:
      return 'pending' as const
  }
}

async function start(): Promise<void> {
  phase.value = 'applying'
  for (const [index, item] of items.entries()) {
    currentIndex.value = index
    item.status = 'running'
    try {
      const r = await featureApply(item.feature.id)
      item.result = r
      item.status = r.ok ? 'ok' : 'failed'
      if (!r.ok) item.error = r.error ?? r.message
    } catch (e) {
      item.status = 'failed'
      item.error = errorText(e)
    }
  }
  phase.value = 'done'
}

function close(): void {
  if (phase.value === 'done') emit('finished')
  emit('close')
}
</script>

<template>
  <ModalDialog title="只应用推荐项" :busy="phase === 'applying'" wide @close="close">
    <template v-if="phase === 'confirm'">
      <p>下面这 {{ items.length }} 项推荐设置还没有开启，确认后会一项一项地应用：</p>
    </template>
    <p v-else-if="phase === 'applying'" class="loading-line" role="status">
      <BusySpinner size="small" />正在应用第 {{ currentIndex + 1 }} 项（共 {{ items.length }} 项）…
    </p>
    <div v-else class="result" :class="failedCount ? 'result-mixed' : 'result-ok'" role="status">
      <p class="result-title">
        {{ failedCount ? `完成了 ${okCount} 项，有 ${failedCount} 项没有改成` : '推荐的设置都应用好了' }}
      </p>
      <p v-if="reboot !== 'none'">
        <strong>{{ rebootLabel[reboot] }}</strong>，之后才能看到全部效果。
      </p>
    </div>

    <ol class="items">
      <li v-for="item in items" :key="item.feature.id" class="item">
        <StatusLamp :state="lamp(item)" />
        <div class="item-body">
          <p class="item-title">{{ item.feature.title }}</p>
          <p class="muted small">{{ item.feature.description }}</p>
          <p v-if="item.feature.reboot !== 'none' && phase === 'confirm'" class="muted small">
            改完{{ rebootLabel[item.feature.reboot] }}
          </p>
          <p v-if="item.status === 'ok'" class="small success-text">
            <AppIcon name="check" :size="14" /> {{ item.result?.message || '已经改好了' }}
          </p>
          <p v-if="item.status === 'failed'" class="small danger-text">没有改成：{{ item.error }}</p>
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

.result-ok {
  background: var(--tone-ok-bg);
  color: var(--tone-ok-text);
}

.result-mixed {
  background: var(--tone-advice-bg);
  color: var(--tone-advice-text);
}

.result-title {
  font-size: var(--text-large);
  font-weight: 600;
}
</style>
