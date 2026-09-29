<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { runCheck, runProfile } from '../api'
import type { CheckResult, FeatureSummary } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BulkApplyDialog from '../components/BulkApplyDialog.vue'
import BusySpinner from '../components/BusySpinner.vue'
import CheckResultCard from '../components/CheckResultCard.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import { statusOrder, type ShownStatus } from '../labels'
import { catalog, findFeature, findSymptom, goHome, goTo, health, openSymptom, openTool, system } from '../state'
import { errorText, parseLink } from '../utils/format'

// 体检：查的时候只读，不改任何东西。不打分；没问题就说一切正常。
// 布局：顶上一条结论（几项要处理、几项能一键修）；左边是要处理的清单，小药箱能直接修的那几项前面有勾选框，
// 勾上的一起修（每一项都记进修改日志、能撤销；修完重新查这几项）；没查出来的单独一组，能重查；正常的收成一行，点开再看。
// 右边是这台电脑的概况（从体检结果里读：C 盘、内存、系统盘）和常见问题的入口。
// 「没查出来」不等于有问题（例如硬盘接在 RAID 控制器上，读不到健康信息）：没有要处理的项目时照样说一切正常。
// 体检以后在别的页面改过或撤销过设置，结果就可能过期了：提示一下，给「重新体检」。

const PROFILE_ID = 'healthcheck'

/** 体检清单一共几项（从目录里读，不写死） */
const checkCount = computed(() => catalog.value?.profiles.find((p) => p.id === PROFILE_ID)?.checkCount ?? null)

type ShownResult = CheckResult & { status: ShownStatus }

const results = ref<CheckResult[] | null>(null)
const running = ref(false)
const error = ref<string | null>(null)
const elapsed = ref(0)
/** 上一次体检用了几秒 */
const lastDuration = ref<number | null>(null)
const previewId = ref<string | null>(null)
/** 从哪一行打开的预览：修完以后重新查这一项 */
let previewFromCheck: string | null = null
let timer: ReturnType<typeof setInterval> | undefined

function isShown(r: CheckResult): r is ShownResult {
  return r.status !== 'na'
}

/** 不适用的不显示；需要人工、建议处理、没查出来在前，正常在后 */
const shown = computed<ShownResult[]>(() =>
  (results.value ?? [])
    .filter(isShown)
    .map((r, i) => ({ r, i }))
    .sort((a, b) => statusOrder[a.r.status] - statusOrder[b.r.status] || a.i - b.i)
    .map((x) => x.r),
)
/** 需要留意的：需要人工、建议处理 */
const attention = computed(() => shown.value.filter((r) => r.status === 'manual' || r.status === 'advice'))
const unknown = computed(() => shown.value.filter((r) => r.status === 'unknown'))
const normal = computed(() => shown.value.filter((r) => r.status === 'ok'))
/** 没有要处理的项目（没查出来的不算） */
const allOk = computed(() => results.value !== null && attention.value.length === 0)

// 导航栏「体检」旁边的数字
watch(
  () => (results.value ? attention.value.length : null),
  (n) => {
    health.attention = n
  },
  { immediate: true },
)

/**
 * 能直接修的：需要留意的结果里链到的修复，去掉重复。只收这台电脑能用、安全、能撤销、推荐做的
 * （和「应用推荐的设置」一样，计划书 6.2）；有代价的（比如缩小休眠文件）照样在那一行上点「预览修复」，看清楚再改。
 * checkIds：修完要重新查的检测
 */
interface FixItem {
  feature: FeatureSummary
  checkIds: string[]
}
function isBulkSafe(f: FeatureSummary): boolean {
  return f.applicable && f.risk === 'safe' && f.reversible && f.recommend === 'recommended'
}
const fixable = computed<FixItem[]>(() => {
  const byId = new Map<string, FixItem>()
  for (const r of attention.value) {
    for (const link of r.links) {
      const l = parseLink(link)
      if (!l || l.kind !== 'feature') continue
      const feature = findFeature(l.id)
      if (!feature || !isBulkSafe(feature)) continue
      const item = byId.get(l.id) ?? { feature, checkIds: [] }
      if (!item.checkIds.includes(r.id)) item.checkIds.push(r.id)
      byId.set(l.id, item)
    }
  }
  return [...byId.values()]
})
/** 能一键修的检测（有勾选框的那几行） */
const fixableChecks = computed(() => new Set(fixable.value.flatMap((i) => i.checkIds)))
/** 勾上的修复（ID）。体检结果一变就重新全部勾上 */
const chosen = ref<string[]>([])
watch(fixable, (list) => {
  chosen.value = list.map((i) => i.feature.id)
})
const chosenFeatures = computed(() => fixable.value.filter((i) => chosen.value.includes(i.feature.id)).map((i) => i.feature))

/** 这一行的修复都勾上了没有 */
function rowSelected(checkId: string): boolean {
  const ids = fixable.value.filter((i) => i.checkIds.includes(checkId)).map((i) => i.feature.id)
  return ids.length > 0 && ids.every((id) => chosen.value.includes(id))
}

function toggleRow(checkId: string, checked: boolean): void {
  const ids = fixable.value.filter((i) => i.checkIds.includes(checkId)).map((i) => i.feature.id)
  const rest = chosen.value.filter((id) => !ids.includes(id))
  chosen.value = checked ? [...rest, ...ids] : rest
}

const bulkOpen = ref(false)

async function refreshChecks(ids: string[]): Promise<void> {
  for (const id of ids) {
    try {
      const fresh = await runCheck(id)
      if (results.value) results.value = results.value.map((r) => (r.id === id ? fresh : r))
    } catch {
      // 复查失败就保留原来的结果，不打扰用户
    }
  }
}

/** 修完以后重新查这几项；没改动东西就不查 */
async function onBulkFinished(changed: boolean): Promise<void> {
  if (!changed || !results.value) return
  const ids = [...new Set(fixable.value.filter((i) => chosen.value.includes(i.feature.id)).flatMap((i) => i.checkIds))]
  await refreshChecks(ids)
}

// ── 没查出来的：单独重查一项 ──

const rechecking = ref<string | null>(null)

async function recheck(id: string): Promise<void> {
  if (rechecking.value) return
  rechecking.value = id
  await refreshChecks([id])
  rechecking.value = null
}

// ── 正常的：收成一行，点开再看 ──

const showNormal = ref(false)
const normalPreview = computed(() => normal.value.slice(0, 10).map((r) => r.title))

function stopTimer(): void {
  if (timer !== undefined) clearInterval(timer)
  timer = undefined
}

async function start(): Promise<void> {
  if (running.value) return
  running.value = true
  error.value = null
  elapsed.value = 0
  showNormal.value = false
  // 开始时就清掉：体检进行中又改了设置的话，会重新标成过期
  health.stale = false
  const startedAt = Date.now()
  timer = setInterval(() => {
    elapsed.value = Math.floor((Date.now() - startedAt) / 1000)
  }, 500)
  try {
    results.value = await runProfile(PROFILE_ID)
    health.lastRunAt = new Date().toISOString()
    lastDuration.value = Math.max(1, Math.round((Date.now() - startedAt) / 1000))
  } catch (e) {
    error.value = errorText(e)
  } finally {
    stopTimer()
    running.value = false
  }
}

function openPreview(featureId: string, checkId: string): void {
  previewId.value = featureId
  previewFromCheck = checkId
}

/** 修完以后重新查一下这一项 */
async function onApplied(): Promise<void> {
  const checkId = previewFromCheck
  if (!checkId || !results.value) return
  await refreshChecks([checkId])
}

// 报告页点了「重新体检」
watch(
  () => health.runRequest,
  () => void start(),
)

onBeforeUnmount(stopTimer)

// ── 结论那一条 ──

/** 「今天 00:19」「9月28日 21:40」 */
const lastRunText = computed(() => {
  if (!health.lastRunAt) return ''
  const d = new Date(health.lastRunAt)
  const time = d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
  const today = new Date()
  const sameDay = d.toDateString() === today.toDateString()
  return sameDay ? `今天 ${time}` : `${d.getMonth() + 1}月${d.getDate()}日 ${time}`
})

const headline = computed(() => {
  if (allOk.value) return unknown.value.length ? `一切正常（有 ${unknown.value.length} 项没查出来）` : '一切正常'
  const n = attention.value.length
  return fixable.value.length ? `有 ${n} 项建议处理，其中 ${fixable.value.length} 项能一键修好` : `有 ${n} 项建议处理`
})

const headlineSub = computed(() => {
  const parts = [lastRunText.value ? `${lastRunText.value} 体检` : '', lastDuration.value ? `用了 ${lastDuration.value} 秒` : '']
  // 「一次查完 N 项」说的是全部项目，这里也按全部算；不适用的（比如台式机的电池）不列出来，说一声
  const total = results.value?.length ?? 0
  const na = total - shown.value.length
  parts.push(na > 0 ? `查了 ${total} 项（${na} 项和这台电脑无关，没列出来）` : `查了 ${total} 项`)
  return parts.filter(Boolean).join(' · ')
})

const stats = computed(() => [
  { label: '建议处理', value: attention.value.length, tone: 'advice' },
  { label: '能一键修', value: fixable.value.length, tone: 'info' },
  { label: '没查出来', value: unknown.value.length, tone: 'unknown' },
  { label: '正常', value: normal.value.length, tone: 'ok' },
])

// ── 右边：这台电脑 ──

function resultOf(id: string): CheckResult | undefined {
  return results.value?.find((r) => r.id === id)
}

function numberFact(r: CheckResult | undefined, key: string): number | null {
  const v = r?.facts[key]
  return typeof v === 'number' && Number.isFinite(v) ? v : null
}

const osName = computed(() => system.value?.osCaption.replace(/^Microsoft\s+/i, '') ?? '')

/** C 盘：还剩多少、一共多少（体检里「C 盘剩余空间」的数据） */
const drive = computed(() => {
  const r = resultOf('disk.system-free-space')
  const free = numberFact(r, 'free_gb')
  const total = numberFact(r, 'total_gb')
  if (free === null || total === null || total <= 0) return null
  const used = Math.min(100, Math.max(0, Math.round(((total - free) / total) * 100)))
  return { free, total, used, low: r?.status === 'advice' || r?.status === 'manual' }
})

const specs = computed(() => {
  const rows: { k: string; v: string }[] = []
  if (osName.value) rows.push({ k: '系统', v: osName.value })
  const mem = numberFact(resultOf('hardware.memory-size'), 'total_gb')
  if (mem !== null) rows.push({ k: '内存', v: `${mem} GB` })
  const disk = resultOf('hardware.system-disk-type')?.resultCode
  const diskName: Record<string, string> = { ssd: '固态硬盘', hdd: '机械硬盘', virtual: '虚拟磁盘' }
  if (disk && diskName[disk]) rows.push({ k: '系统盘', v: diskName[disk] })
  return rows
})

// ── 右边：常见问题的入口 ──

const COMMON: { id: string; label: string }[] = [
  { id: 'network', label: '上不了网' },
  { id: 'disk-full', label: 'C 盘满了' },
  { id: 'printer-share', label: '打印机连不上' },
  { id: 'slow-pc', label: '电脑卡' },
  { id: 'no-sound', label: '没声音' },
  { id: 'bluescreen', label: '蓝屏' },
]
const common = computed(() => (catalog.value ? COMMON.filter((c) => findSymptom(c.id)) : []))
const symptomCount = computed(() => catalog.value?.symptoms.length ?? 0)
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div class="page-head-main">
        <h1 class="page-title" tabindex="-1">体检</h1>
        <p class="page-lead">
          一次查完{{ checkCount ? ` ${checkCount} 项` : '' }}，只看不改；能修的集中在一起，勾上一次修好。
        </p>
      </div>
      <div class="page-actions">
        <button type="button" class="btn btn-secondary" @click="goTo('report')">
          <AppIcon name="report" :size="18" />生成诊断报告
        </button>
        <button v-if="results && !running" type="button" class="btn btn-primary" @click="start">
          <AppIcon name="refresh" :size="18" />重新体检
        </button>
      </div>
    </header>

    <!-- 结论：还没体检 / 正在体检 / 出错 / 查完了 -->
    <section class="card summary" aria-live="polite">
      <template v-if="running">
        <BusySpinner size="large" />
        <div class="summary-text">
          <p class="summary-title">正在体检…</p>
          <p class="muted">已经用了 {{ elapsed }} 秒。体检只读取信息，不会改动电脑。</p>
        </div>
      </template>

      <template v-else-if="error">
        <span class="summary-icon tone-manual"><AppIcon name="warning" :size="28" /></span>
        <div class="summary-text">
          <p class="summary-title">体检没能完成</p>
          <p class="muted">{{ error }}</p>
        </div>
        <button type="button" class="btn btn-primary" @click="start">再试一次</button>
      </template>

      <template v-else-if="results === null">
        <span class="summary-icon tone-info"><AppIcon name="health" :size="28" /></span>
        <div class="summary-text">
          <p class="summary-title">把这台电脑常见的毛病查一遍</p>
          <p class="muted">大约半分钟{{ checkCount ? `，一共 ${checkCount} 项` : '' }}。查完告诉你哪些正常、哪些需要留意；没坏的别修，看不懂的就跳过。</p>
        </div>
        <button type="button" class="btn btn-primary btn-big" @click="start">
          <AppIcon name="health" :size="22" />开始体检
        </button>
      </template>

      <template v-else>
        <span class="summary-icon" :class="allOk ? 'tone-ok' : 'tone-advice'">
          <AppIcon :name="allOk ? 'check' : 'warning'" :size="28" />
        </span>
        <div class="summary-text">
          <p class="summary-title" :class="{ ok: allOk }">{{ headline }}</p>
          <p class="muted small">{{ headlineSub }}</p>
        </div>
        <ul class="stats" aria-label="体检结果统计">
          <li v-for="s in stats" :key="s.label" class="stat" :class="`tone-${s.tone}`">
            <span class="stat-value">{{ s.value }}</span>
            <span class="stat-label">{{ s.label }}</span>
          </li>
        </ul>
      </template>
    </section>

    <div v-if="health.stale && results && !running" class="banner banner-warning" role="status">
      <AppIcon name="warning" :size="20" />
      <div class="stale">
        <p class="banner-title">下面的体检结果可能已经过期</p>
        <p>体检以后，你在「按症状修」「常用设置」或「修改日志」里改过或撤销过设置。重新体检一次，看到的才是现在的情况。</p>
      </div>
      <button type="button" class="btn btn-secondary btn-small" @click="start">重新体检</button>
    </div>

    <div class="two-col">
      <div class="main-col">
        <template v-if="results && !running">
          <section v-if="attention.length" class="card list-card" aria-labelledby="health-todo-title">
            <div class="list-head">
              <h2 id="health-todo-title" class="list-title">需要处理的（{{ attention.length }}）</h2>
              <button
                v-if="fixable.length"
                type="button"
                class="btn btn-primary btn-small"
                :disabled="!chosenFeatures.length"
                @click="bulkOpen = true"
              >
                <AppIcon name="check" :size="16" />一键修好勾上的 {{ chosenFeatures.length }} 项
              </button>
            </div>
            <p v-if="fixable.length" class="list-note muted small">
              标着「能一键修」的是安全、能撤销的改动，默认都勾上；每一项都记进「修改日志」，随时能恢复原状。别的点「预览修复」看清楚再改。
            </p>
            <ul class="rows">
              <CheckResultCard
                v-for="r in attention"
                :key="r.id"
                :result="r"
                :selectable="fixableChecks.has(r.id)"
                :selected="rowSelected(r.id)"
                @toggle="(checked) => toggleRow(r.id, checked)"
                @preview="(featureId) => openPreview(featureId, r.id)"
              />
            </ul>
          </section>

          <section v-if="unknown.length" class="card list-card" aria-labelledby="health-unknown-title">
            <div class="list-head">
              <h2 id="health-unknown-title" class="list-title">没查出来（{{ unknown.length }}）</h2>
              <span class="muted small">没查出来不一定有毛病，可以过一会儿再查一次</span>
            </div>
            <ul class="rows">
              <CheckResultCard
                v-for="r in unknown"
                :key="r.id"
                :result="r"
                recheck
                :rechecking="rechecking === r.id"
                @recheck="recheck(r.id)"
                @preview="(featureId) => openPreview(featureId, r.id)"
              />
            </ul>
          </section>

          <section v-if="normal.length" class="card list-card" aria-labelledby="health-normal-title">
            <div class="list-head normal-head">
              <!-- 标题里放按钮（按钮里不能放标题）：读屏软件照样能按标题跳过来 -->
              <h2 id="health-normal-title" class="list-title normal-title">
                <button
                  type="button"
                  class="normal-toggle"
                  :aria-expanded="showNormal"
                  aria-controls="health-normal-list"
                  @click="showNormal = !showNormal"
                >
                  <AppIcon :name="showNormal ? 'chevron-down' : 'chevron-right'" :size="18" />
                  {{ allOk ? '查过的项目' : '都正常的' }}（{{ normal.length }}）
                </button>
              </h2>
            </div>
            <div v-if="!showNormal" class="normal-preview">
              <span v-for="t in normalPreview" :key="t" class="ok-chip">{{ t }}</span>
              <span v-if="normal.length > normalPreview.length" class="muted small">等 {{ normal.length }} 项</span>
            </div>
            <ul v-show="showNormal" id="health-normal-list" class="rows normal-grid">
              <CheckResultCard
                v-for="r in normal"
                :key="r.id"
                :result="r"
                compact
                @preview="(featureId) => openPreview(featureId, r.id)"
              />
            </ul>
          </section>
        </template>

        <section v-else-if="!running" class="card intro" aria-labelledby="health-intro-title">
          <h2 id="health-intro-title" class="list-title">体检会查这些</h2>
          <ul class="intro-list">
            <li><AppIcon name="disk" :size="18" />C 盘空间、硬盘健康和读写错误</li>
            <li><AppIcon name="refresh" :size="18" />Windows 更新有没有被关掉、是不是在等重启</li>
            <li><AppIcon name="wifi" :size="18" />网络通不通、失效的代理、hosts 文件</li>
            <li><AppIcon name="shield" :size="18" />防火墙、用户账户控制、程序有没有被「劫持」</li>
            <li><AppIcon name="cpu" :size="18" />设备和驱动、内存、显卡、屏幕分辨率</li>
          </ul>
          <p class="muted small">只读取信息，不会改动电脑。查完能修的会列出来，修不修由你决定。</p>
        </section>
      </div>

      <aside class="side" aria-label="这台电脑和常见问题">
        <section class="card side-card" aria-labelledby="pc-title">
          <div class="side-head">
            <h2 id="pc-title" class="list-title">这台电脑</h2>
            <button type="button" class="btn btn-link small" @click="openTool('system.hardware-info')">看详细配置</button>
          </div>
          <div v-if="drive" class="drive">
            <div class="drive-line">
              <span class="muted">C 盘</span>
              <span :class="{ 'danger-text': drive.low }">还剩 {{ drive.free }} GB，共 {{ drive.total }} GB</span>
            </div>
            <div
              class="bar"
              role="meter"
              :aria-valuenow="drive.used"
              aria-valuemin="0"
              aria-valuemax="100"
              :aria-label="`C 盘用了 ${drive.used}%`"
            >
              <span class="bar-fill" :class="{ low: drive.low }" :style="{ width: `${drive.used}%` }"></span>
            </div>
          </div>
          <dl v-if="specs.length" class="specs">
            <template v-for="row in specs" :key="row.k">
              <dt>{{ row.k }}</dt>
              <dd>{{ row.v }}</dd>
            </template>
          </dl>
          <p v-if="!results" class="muted small">体检以后，这里会显示 C 盘还剩多少、内存多大、系统盘是什么硬盘。</p>
        </section>

        <section v-if="common.length" class="card side-card" aria-labelledby="faq-title">
          <div>
            <h2 id="faq-title" class="list-title">遇到具体问题了？</h2>
            <p class="muted small">按症状一步步查原因。</p>
          </div>
          <div class="chips">
            <button v-for="c in common" :key="c.id" type="button" class="chip chip-soft" @click="openSymptom(c.id)">
              {{ c.label }}
            </button>
          </div>
          <button type="button" class="btn btn-link small all-link" @click="goHome('symptoms')">
            全部 {{ symptomCount }} 个症状<AppIcon name="chevron-right" :size="14" />
          </button>
        </section>
      </aside>
    </div>

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
    <BulkApplyDialog
      v-if="bulkOpen"
      :features="chosenFeatures"
      title="修好体检发现的问题"
      intro="下面这 {n} 项确认后会一项一项地修："
      done-title="选中的问题都修好了"
      verb="修复"
      @close="bulkOpen = false"
      @finished="onBulkFinished"
    />
  </div>
</template>

<style scoped>
.summary {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 16px 22px;
  padding: 22px 26px;
}

.summary-icon {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  border-radius: 50%;
}

.summary-text {
  display: flex;
  flex: 1 1 320px;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.summary-title {
  font-size: 21px;
  font-weight: 600;
  line-height: 1.4;
}

.summary-title.ok {
  color: var(--tone-ok-text);
}

.tone-ok {
  background: var(--tone-ok-bg);
  color: var(--tone-ok-text);
}

.tone-advice {
  background: var(--tone-advice-bg);
  color: var(--tone-advice-text);
}

.tone-manual {
  background: var(--tone-manual-bg);
  color: var(--tone-manual-text);
}

.tone-unknown {
  background: var(--tone-unknown-bg);
  color: var(--tone-unknown-text);
}

.tone-info {
  background: var(--tone-info-bg);
  color: var(--tone-info-text);
}

.stats {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  list-style: none;
}

.stat {
  display: flex;
  flex-direction: column;
  width: 100px;
  padding: 9px 14px;
  border-radius: var(--radius);
}

.stat-value {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.3;
}

.stat-label {
  font-size: 12.5px;
}

.stale {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
}

.main-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

.rows {
  list-style: none;
}

.list-note {
  padding: 10px 20px 0;
}

.normal-head {
  padding: 10px 14px;
}

.normal-title {
  font-size: var(--text-base);
}

.normal-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  font: inherit;
  cursor: pointer;
}

.normal-toggle:hover {
  background: var(--color-surface-2);
}

.normal-preview {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 12px 20px 16px;
}

.ok-chip {
  padding: 2px 10px;
  border-radius: 12px;
  background: var(--tone-ok-bg);
  color: var(--tone-ok-text);
  font-size: 12.5px;
}

/* 正常的项目展开后排两列，紧凑一些 */
.normal-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  padding: 4px 4px 8px;
}

.normal-grid :deep(.result) {
  border-top: none;
}

@media (max-width: 1280px) {
  .normal-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

.intro {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px 22px;
}

.intro-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  list-style: none;
}

.intro-list li {
  display: flex;
  align-items: center;
  gap: 10px;
}

.intro-list :deep(.icon) {
  color: var(--color-primary);
}

.side-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 18px 20px;
}

.side-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.drive {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.drive-line {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: var(--text-small);
}

.bar {
  height: 8px;
  overflow: hidden;
  border-radius: 4px;
  background: var(--tone-unknown-bg);
}

.bar-fill {
  display: block;
  height: 100%;
  border-radius: 4px;
  background: var(--color-primary);
}

.bar-fill.low {
  background: var(--tone-manual-dot);
}

.specs {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 6px 16px;
  margin: 0;
  font-size: var(--text-small);
}

.specs dt {
  color: var(--color-text-muted);
}

.specs dd {
  margin: 0;
  text-align: right;
}

.all-link {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
</style>
