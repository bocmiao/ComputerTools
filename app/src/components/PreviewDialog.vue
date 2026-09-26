<script setup lang="ts">
import { computed, onMounted, ref, useId } from 'vue'
import { featureApply, featurePreview } from '../api'
import type { ApplyResult, Preview } from '../api/types'
import {
  rebootLabel,
  riskLabel,
  riskTone,
  verifiedLabel,
  verifiedTone,
} from '../labels'
import { vAutofocus } from '../utils/dialogs'
import { errorText } from '../utils/format'
import BusySpinner from './BusySpinner.vue'
import ModalDialog from './ModalDialog.vue'
import PathText from './PathText.vue'
import TagPill from './TagPill.vue'

// 预览与执行：先看清楚要改哪些地方、能不能撤销，确认后才执行，最后显示复查结果。

const props = defineProps<{ featureId: string }>()
const emit = defineEmits<{ close: []; applied: [result: ApplyResult] }>()

type Phase = 'loading' | 'load-error' | 'ready' | 'applying' | 'done'

const phase = ref<Phase>('loading')
const preview = ref<Preview | null>(null)
const result = ref<ApplyResult | null>(null)
const loadError = ref('')
const applyError = ref('')
const acknowledged = ref(false)
const changesTitleId = useId()
const notesTitleId = useId()

const feature = computed(() => preview.value?.feature ?? null)
/** 🔴「谨慎」级的功能要多勾一下确认 */
const needsAck = computed(() => feature.value?.risk === 'danger')
const canApply = computed(() => phase.value === 'ready' && (!needsAck.value || acknowledged.value))
const title = computed(() => feature.value?.title ?? '预览修改')
/** ok 且没有写日志：这一项本来就是好的，什么也没改 */
const resultTitle = computed(() => {
  const r = result.value
  if (!r) return ''
  if (!r.ok) return '没有改成'
  return r.entryIds.length === 0 ? '不用改' : '已经改好了'
})
const allUnchanged = computed(
  () => !!preview.value && preview.value.changes.length > 0 && preview.value.changes.every((c) => c.current === c.planned),
)

async function load(): Promise<void> {
  phase.value = 'loading'
  loadError.value = ''
  try {
    preview.value = await featurePreview(props.featureId)
    phase.value = 'ready'
  } catch (e) {
    loadError.value = errorText(e)
    phase.value = 'load-error'
  }
}

async function apply(): Promise<void> {
  if (!canApply.value) return
  phase.value = 'applying'
  applyError.value = ''
  try {
    const r = await featureApply(props.featureId)
    result.value = r
    emit('applied', r)
  } catch (e) {
    applyError.value = errorText(e)
  }
  phase.value = 'done'
}

onMounted(load)
</script>

<template>
  <ModalDialog :title="title" :busy="phase === 'applying'" wide @close="emit('close')">
    <!-- 读取预览 -->
    <p v-if="phase === 'loading'" class="loading-line" role="status"><BusySpinner />正在读取要改动的地方…</p>

    <div v-else-if="phase === 'load-error'" class="banner banner-error" role="alert">
      <div>
        <p class="banner-title">没能读出预览</p>
        <p>{{ loadError }}</p>
      </div>
    </div>

    <!-- 预览 -->
    <template v-else-if="(phase === 'ready' || phase === 'applying') && preview && feature">
      <p class="lead">{{ feature.description }}</p>
      <div class="tags">
        <TagPill :tone="riskTone[feature.risk]" dot>风险：{{ riskLabel[feature.risk] }}</TagPill>
        <TagPill v-if="feature.reboot !== 'none'" tone="neutral">改完{{ rebootLabel[feature.reboot] }}</TagPill>
        <TagPill v-if="feature.subjective" tone="neutral">个人偏好</TagPill>
      </div>

      <section class="block" :aria-labelledby="changesTitleId">
        <h3 :id="changesTitleId" class="block-title">会改动这些地方</h3>
        <div v-if="preview.changes.length" class="table-wrap">
          <table class="table changes">
            <colgroup>
              <col class="col-target" />
              <col class="col-value" />
              <col class="col-value" />
            </colgroup>
            <thead>
              <tr>
                <th scope="col">位置</th>
                <th scope="col">现在</th>
                <th scope="col">改成</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(c, i) in preview.changes" :key="i" :class="{ unchanged: c.current === c.planned }">
                <td><PathText :text="c.target" /></td>
                <td>{{ c.current }}</td>
                <td>
                  {{ c.planned }}
                  <span v-if="c.current === c.planned" class="muted small">（已经是这样）</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-else class="muted">没有需要改动的地方。</p>
        <p v-if="allUnchanged" class="muted small">这些地方现在已经是目标状态了，执行也不会有变化。</p>
      </section>

      <ul class="facts">
        <li>
          <span class="fact-label">还原点</span>
          <span v-if="preview.willCreateRestorePoint">执行前会先创建一个系统还原点，作为最后一道保险（可能要等几十秒）。</span>
          <span v-else-if="feature.risk === 'safe'">这一项风险低，不需要创建还原点。</span>
          <span v-else>这次不需要创建还原点。</span>
        </li>
        <li>
          <span class="fact-label">能否撤销</span>
          <span v-if="feature.reversible">能撤销。改完以后，可以在「修改日志」里随时恢复原状。</span>
          <strong v-else class="danger-text">不能撤销：{{ feature.irreversibleReason ?? '这一项改了就退不回去。' }}</strong>
        </li>
      </ul>

      <section v-if="preview.notes.length" class="block" :aria-labelledby="notesTitleId">
        <h3 :id="notesTitleId" class="block-title">注意</h3>
        <ul class="notes">
          <li v-for="(n, i) in preview.notes" :key="i">{{ n }}</li>
        </ul>
      </section>

      <label v-if="needsAck" class="ack">
        <input v-model="acknowledged" type="checkbox" :disabled="phase === 'applying'" />
        <span>这一项风险较高。我已经看过上面的说明，仍然要执行。</span>
      </label>
    </template>

    <!-- 执行结果 -->
    <template v-else-if="phase === 'done'">
      <div v-if="result" class="result" :class="result.ok ? 'result-ok' : 'result-fail'" role="status">
        <p class="result-title">{{ resultTitle }}</p>
        <p v-if="result.message">{{ result.message }}</p>
      </div>
      <div v-else class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能执行</p>
          <p>{{ applyError }}</p>
        </div>
      </div>

      <dl v-if="result" class="kv">
        <dt>修好了没</dt>
        <dd><TagPill :tone="verifiedTone[result.verified]">{{ verifiedLabel[result.verified] }}</TagPill></dd>
        <template v-if="result.reboot !== 'none'">
          <dt>还要做</dt>
          <dd>
            <strong>{{ rebootLabel[result.reboot] }}</strong>
            <span class="muted">，之后才能看到效果。</span>
          </dd>
        </template>
        <template v-if="result.error">
          <dt>出错信息</dt>
          <dd class="danger-text">{{ result.error }}</dd>
        </template>
      </dl>

      <ul v-if="result && result.notes.length" class="notes">
        <li v-for="(n, i) in result.notes" :key="i">{{ n }}</li>
      </ul>

      <p v-if="result && result.entryIds.length" class="muted small">
        这次的改动已经记进「修改日志」，想改回去，可以在那里恢复原状。
      </p>
    </template>

    <template #footer>
      <template v-if="phase === 'ready' || phase === 'applying'">
        <span v-if="phase === 'applying'" class="loading-line" role="status">
          <BusySpinner size="small" />正在执行，请不要关闭小药箱…
        </span>
        <button type="button" class="btn btn-secondary" :disabled="phase === 'applying'" @click="emit('close')">取消</button>
        <button type="button" class="btn btn-primary" :disabled="!canApply" @click="apply">确认执行</button>
      </template>
      <template v-else-if="phase === 'load-error'">
        <button type="button" class="btn btn-secondary" @click="emit('close')">关闭</button>
        <button type="button" class="btn btn-primary" @click="load">重试</button>
      </template>
      <button v-else-if="phase === 'done'" type="button" class="btn btn-primary" v-autofocus @click="emit('close')">
        完成
      </button>
      <button v-else type="button" class="btn btn-secondary" @click="emit('close')">取消</button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.lead {
  font-size: var(--text-base);
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.block-title {
  font-size: var(--text-base);
}

.changes {
  table-layout: fixed;
}

.changes .col-target {
  width: 50%;
}

.changes .col-value {
  width: 25%;
}

.changes .unchanged td {
  color: var(--color-text-muted);
}

.facts {
  display: flex;
  flex-direction: column;
  gap: 8px;
  list-style: none;
  padding: 12px 16px;
  background: var(--color-surface-2);
  border-radius: var(--radius);
}

.facts li {
  display: grid;
  grid-template-columns: 5em 1fr;
  gap: 12px;
}

.fact-label {
  color: var(--color-text-muted);
}

.notes {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 1.3em;
}

.ack {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  padding: 12px 16px;
  border: 1px solid var(--tone-manual-dot);
  border-radius: var(--radius);
  background: var(--tone-manual-bg);
  cursor: pointer;
}

.ack input {
  width: 18px;
  height: 18px;
  margin-top: 3px;
  flex: none;
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

.result-fail {
  background: var(--tone-advice-bg);
  color: var(--tone-advice-text);
}

.result-title {
  font-size: var(--text-large);
  font-weight: 600;
}
</style>
