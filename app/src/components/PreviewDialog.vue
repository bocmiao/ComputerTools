<script setup lang="ts">
import { computed, onMounted, ref, useId } from 'vue'
import { featureApply, featurePreview } from '../api'
import type { ApplyResult, Preview } from '../api/types'
import { applyOutcome, applyOutcomeTitle, verifiedLabel, verifiedTone } from '../labels'
import { vAutofocus } from '../utils/dialogs'
import { errorText } from '../utils/format'
import AppIcon from './AppIcon.vue'
import BusySpinner from './BusySpinner.vue'
import ExplorerRestart from './ExplorerRestart.vue'
import ModalDialog from './ModalDialog.vue'
import PathText from './PathText.vue'
import TagPill from './TagPill.vue'

// 改之前的确认框：用大白话把要做的事说清楚，确认后才执行，最后显示复查结果。
// - 先说这一项是做什么的（功能说明），现在是什么情况（脚本类功能、写了复查检测的功能才有）；
// - 「点了以后会这样做」按顺序写：先建还原点（要建的话）、改什么、改完要重启什么、改完再查一遍；
// - 能不能撤销只说一次；不能撤销的说清楚万一出事怎么办；
// - 注册表路径、服务名这些技术细节收进「给懂哥看」，默认不展开；
// - 按钮写清楚会做什么，不写「确认执行」。

const props = defineProps<{ featureId: string }>()
const emit = defineEmits<{ close: []; applied: [result: ApplyResult] }>()

type Phase = 'loading' | 'load-error' | 'ready' | 'applying' | 'done'

const phase = ref<Phase>('loading')
const preview = ref<Preview | null>(null)
const result = ref<ApplyResult | null>(null)
const loadError = ref('')
const applyError = ref('')
const acknowledged = ref(false)
const stepsTitleId = useId()

const feature = computed(() => preview.value?.feature ?? null)
/** 这台电脑能不能用这一项（系统版本不对等）；不能用时不给执行 */
const applicable = computed(() => feature.value?.applicable !== false)
const notApplicableReason = computed(() => feature.value?.notApplicableReason ?? '这台电脑的系统不支持这一项。')
/** 🔴「谨慎」级的功能要多勾一下确认 */
const needsAck = computed(() => feature.value?.risk === 'danger' && applicable.value)
const canApply = computed(
  () => phase.value === 'ready' && applicable.value && !allUnchanged.value && (!needsAck.value || acknowledged.value),
)
const title = computed(() => feature.value?.title ?? '改之前先看一看')
/** 后端在 notes 里也写了一句「不能执行：原因」，上面已经单独显示原因了，这里不再重复 */
const notes = computed(() => {
  const all = preview.value?.notes ?? []
  if (applicable.value) return all
  return all.filter((n) => n !== `不能执行：${feature.value?.notApplicableReason ?? ''}`)
})

/** 逐个位置都已经是目标状态：执行也不会有变化 */
const allUnchanged = computed(
  () => !!preview.value && preview.value.changes.length > 0 && preview.value.changes.every((c) => c.current === c.planned),
)
/** 还要改的位置 */
const pendingChanges = computed(() => (preview.value?.changes ?? []).filter((c) => c.current !== c.planned))

/** 改完要做什么（和 labels.ts 的 rebootLabel 说法一致，这里多一句会怎样） */
const rebootStep = computed(() => {
  switch (feature.value?.reboot) {
    case 'explorer':
      return { title: '改完重启资源管理器才生效', detail: '改好以后会问你现在重启还是等会儿。重启时桌面和任务栏会闪一下，打开的文件夹窗口会关掉。' }
    case 'logoff':
      return { title: '改完注销再登录才生效', detail: '先把正在写的文档存好，注销再登录以后就能看到效果。' }
    case 'reboot':
      return { title: '改完重启电脑才生效', detail: '不用马上重启，下次开机就生效。' }
    default:
      return null
  }
})

/** 「点了以后会这样做」的每一步 */
const steps = computed(() => {
  const f = feature.value
  const p = preview.value
  if (!f || !p) return []
  const list: { title: string; detail: string }[] = []
  if (p.willCreateRestorePoint) {
    list.push({ title: '先建一个系统还原点', detail: '万一改完电脑出了问题，能用它退回来。大约要等几十秒。' })
  }
  if (p.scripted) {
    list.push({ title: f.title, detail: '由小药箱照上面说的做，用的是 Windows 自带的办法。做的时候不要关掉小药箱。' })
  } else {
    const n = pendingChanges.value.length
    list.push({
      title: n > 0 ? `改 ${n} 个系统设置` : '改系统设置',
      detail: '改的是 Windows 自己的设置，和在系统设置里手动改一样。具体是哪些，展开下面的「给懂哥看」。',
    })
  }
  if (rebootStep.value) list.push(rebootStep.value)
  list.push({ title: '改完再查一遍', detail: '告诉你改好了没有；改的每一处都记进「修改日志」。' })
  return list
})

/** 主按钮上写的字：说清楚会做什么 */
const applyLabel = computed(() => {
  const f = feature.value
  if (!f) return '开始改'
  const action = f.title.length <= 14 ? f.title : '开始改'
  return preview.value?.willCreateRestorePoint ? `建还原点，${action}` : action
})

/** 执行结果按四种分开显示：已经改好 / 改了但没确认生效 / 不用改 / 没改成 */
const outcome = computed(() => (result.value ? applyOutcome(result.value) : null))
const resultTitle = computed(() => (outcome.value ? applyOutcomeTitle[outcome.value] : ''))
/** 后端的说明和标题说的是一回事（例如都是「已经改好了」）时不重复显示 */
const resultMessage = computed(() => {
  const m = result.value?.message.trim() ?? ''
  return m.replace(/[。.！!]+$/, '') === resultTitle.value ? '' : m
})
/** 真改了东西、而且能撤销，才提示可以去修改日志里恢复 */
const canUndoLater = computed(
  () => !!result.value && result.value.ok && result.value.entryIds.length > 0 && feature.value?.reversible === true,
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
    <p v-if="phase === 'loading'" class="loading-line" role="status"><BusySpinner />正在查看要改的地方…</p>

    <div v-else-if="phase === 'load-error'" class="banner banner-error" role="alert">
      <div>
        <p class="banner-title">没能读出要改的地方</p>
        <p>{{ loadError }}</p>
      </div>
    </div>

    <!-- 预览 -->
    <template v-else-if="(phase === 'ready' || phase === 'applying') && preview && feature">
      <p class="lead">{{ feature.description }}</p>

      <div v-if="!applicable" class="banner banner-warning" role="note">
        <AppIcon name="warning" :size="20" />
        <div>
          <p class="banner-title">这台电脑用不了这一项</p>
          <p>{{ notApplicableReason }}</p>
        </div>
      </div>

      <template v-else>
        <p v-if="preview.current" class="now">
          <span class="now-label">现在</span>
          <span>{{ preview.current }}</span>
        </p>

        <p v-if="allUnchanged" class="banner banner-ok" role="note">
          <AppIcon name="check" :size="20" />
          <span>这台电脑已经是这样了，不用再改。</span>
        </p>

        <section v-else class="block" :aria-labelledby="stepsTitleId">
          <h3 :id="stepsTitleId" class="block-title">点了以后会这样做</h3>
          <ol class="steps">
            <li v-for="(s, i) in steps" :key="i" class="step">
              <span class="step-no" aria-hidden="true">{{ i + 1 }}</span>
              <div class="step-text">
                <p class="step-title">{{ s.title }}</p>
                <p class="step-detail">{{ s.detail }}</p>
              </div>
            </li>
          </ol>
        </section>

        <!-- 能不能撤销：只说一次 -->
        <p v-if="feature.reversible" class="undo undo-yes">
          <AppIcon name="undo" :size="18" />
          <span>能撤销：改完以后，在「修改日志」里点「撤销」就改回来。</span>
        </p>
        <div v-else class="banner banner-warning undo-no" role="note">
          <AppIcon name="warning" :size="20" />
          <div>
            <p class="banner-title">改了就不能在修改日志里撤销</p>
            <p>{{ feature.irreversibleReason ?? '这一项改了就退不回去。' }}</p>
            <p v-if="preview.willCreateRestorePoint">万一改完出了问题：用第 1 步建的还原点退回去（工具箱 → 系统还原）。</p>
          </div>
        </div>

        <section v-if="notes.length" class="block">
          <h3 class="block-title">还要知道</h3>
          <ul class="notes">
            <li v-for="(n, i) in notes" :key="i">{{ n }}</li>
          </ul>
        </section>

        <details v-if="preview.changes.length" class="geek">
          <summary>给懂哥看：具体改哪些值</summary>
          <div class="table-wrap">
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
        </details>

        <label v-if="needsAck" class="ack">
          <input v-model="acknowledged" type="checkbox" :disabled="phase === 'applying'" />
          <span>这一项风险比较高。我已经看过上面的说明，还是要改。</span>
        </label>
      </template>
    </template>

    <!-- 执行结果 -->
    <template v-else-if="phase === 'done'">
      <div v-if="result && outcome" class="result" :class="`result-${outcome}`" role="status">
        <p class="result-title">{{ resultTitle }}</p>
        <p v-if="resultMessage">{{ resultMessage }}</p>
      </div>
      <div v-else class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能改</p>
          <p>{{ applyError }}</p>
        </div>
      </div>

      <!-- 没改成（已经退回）时，「修好了没」「还要做」都没有意义，不显示 -->
      <dl v-if="result && (result.ok || result.error)" class="kv">
        <template v-if="result.ok">
          <dt>修好了没</dt>
          <dd><TagPill :tone="verifiedTone[result.verified]">{{ verifiedLabel[result.verified] }}</TagPill></dd>
        </template>
        <template v-if="result.ok && result.reboot !== 'none' && rebootStep">
          <dt>还要做</dt>
          <dd>
            <strong>{{ rebootStep.title.replace(/^改完/, '') }}</strong>
            <!-- 只是重启资源管理器的话，直接给按钮 -->
            <ExplorerRestart v-if="result.reboot === 'explorer'" />
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

      <p v-if="canUndoLater" class="muted small">
        这次的改动已经记进「修改日志」，想改回去，在那里点「撤销」。
      </p>
    </template>

    <template #footer>
      <template v-if="phase === 'ready' || phase === 'applying'">
        <span v-if="phase === 'applying'" class="loading-line" role="status">
          <BusySpinner size="small" />正在改，请不要关掉小药箱…
        </span>
        <button type="button" class="btn btn-secondary" :disabled="phase === 'applying'" @click="emit('close')">
          {{ applicable && !allUnchanged ? '先不改' : '关闭' }}
        </button>
        <button v-if="applicable && !allUnchanged" type="button" class="btn btn-primary" :disabled="!canApply" @click="apply">
          {{ applyLabel }}
        </button>
      </template>
      <template v-else-if="phase === 'load-error'">
        <button type="button" class="btn btn-secondary" @click="emit('close')">关闭</button>
        <button type="button" class="btn btn-primary" @click="load">重试</button>
      </template>
      <button v-else-if="phase === 'done'" v-autofocus type="button" class="btn btn-primary" @click="emit('close')">
        完成
      </button>
      <button v-else type="button" class="btn btn-secondary" @click="emit('close')">取消</button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.lead {
  font-size: var(--text-base);
  line-height: 1.7;
}

.now {
  display: flex;
  gap: 12px;
  padding: 10px 14px;
  border-radius: var(--radius);
  background: var(--color-surface-2);
  font-size: var(--text-small);
}

.now-label {
  flex: none;
  color: var(--color-text-muted);
}

.block {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.block-title {
  font-size: var(--text-base);
}

.steps {
  display: flex;
  flex-direction: column;
  gap: 10px;
  list-style: none;
}

.step {
  display: flex;
  gap: 12px;
}

.step-no {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--color-primary);
  color: var(--color-primary-text);
  font-size: 13px;
  font-weight: 700;
}

.step-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.step-title {
  font-weight: 600;
}

.step-detail {
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

.undo {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-small);
}

.undo-yes {
  color: var(--color-success-text);
}

.undo-no p + p {
  margin-top: 4px;
}

.notes {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 1.3em;
  font-size: var(--text-small);
}

.geek {
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
}

.geek summary {
  padding: 8px 12px;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  cursor: pointer;
}

.geek .table-wrap {
  margin: 0 12px 12px;
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

/* 四种结果：已经改好（绿）、不用改（蓝）、改了但没确认生效（橙）、没改成（红） */
.result-done {
  background: var(--tone-ok-bg);
  color: var(--tone-ok-text);
}

.result-unchanged {
  background: var(--tone-info-bg);
  color: var(--tone-info-text);
}

.result-unverified {
  background: var(--tone-advice-bg);
  color: var(--tone-advice-text);
}

.result-failed {
  background: var(--tone-manual-bg);
  color: var(--tone-manual-text);
}

.result-title {
  font-size: var(--text-large);
  font-weight: 600;
}
</style>
