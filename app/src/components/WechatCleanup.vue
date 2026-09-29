<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef, watch } from 'vue'
import { isTauri, wechatClean, wechatScan } from '../api'
import type { WechatAccount, WechatCleanResult, WechatReport } from '../api/types'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'
import ConfirmDialog from './ConfirmDialog.vue'

// 微信占 C 盘：找出这台电脑上微信 3.x、4.x 的账号（后端只给编号、版本、在哪个盘、各类文件的个数和大小，没有账号名和路径），
// 把勾选的放进回收站（后端 src-tauri/src/wechat.rs，挑哪些文件的规则在 medkit_core::wechat，照 CleanMyWechat）：
// - 缓存和临时文件：只算几天以前的，默认勾上；
// - 聊天里的图片、视频、文件：只算 N 天以前的，默认不勾，要自己勾；
// - 放进回收站，和在资源管理器里按 Delete 一样；看过微信里都正常以后再清空回收站，才腾出地方。

// 标题的级别：在「C 盘满了」页面上和「检查步骤」同级（h2），放在一组小工具里时用 h3。
// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件。
const props = withDefaults(defineProps<{ level?: 'h2' | 'h3'; hideTitle?: boolean }>(), {
  level: 'h2',
  hideTitle: false,
})

const DAY_OPTIONS = [90, 180, 365, 730]

const report = ref<WechatReport | null>(null)
const days = ref(365)
const loading = ref(false)
const error = ref('')
const chosen = ref<number[]>([])
const wantCache = ref(true)
const wantChat = ref(false)
const confirming = ref(false)
const cleaning = ref(false)
const outcome = ref<{ ok: boolean; text: string } | null>(null)
const message = useTemplateRef<HTMLElement>('message')

function accountName(a: WechatAccount): string {
  const drive = a.drive ? `${a.drive} 盘` : '不知道在哪个盘'
  const last = a.lastFileSecs ? `，最近收到文件：${new Date(a.lastFileSecs * 1000).toLocaleDateString('zh-CN')}` : ''
  return `账号 ${a.id + 1}（微信 ${a.version}，${drive}${last}）`
}

function sizeText(files: number, bytes: number): string {
  return files ? `${files} 个文件，${formatBytes(bytes)}` : '没有'
}

const selected = computed(() => report.value?.accounts.filter((a) => chosen.value.includes(a.id)) ?? [])
const total = computed(() => {
  let files = 0
  let bytes = 0
  for (const a of selected.value) {
    if (wantCache.value) {
      files += a.cacheFiles
      bytes += a.cacheBytes
    }
    if (wantChat.value) {
      files += a.chatFiles
      bytes += a.chatBytes
    }
  }
  return { files, bytes }
})
const kinds = computed(() => {
  const r = report.value
  if (!r) return ''
  const parts: string[] = []
  if (wantCache.value) parts.push(`缓存和临时文件（${r.cacheDays} 天以前的）`)
  if (wantChat.value) parts.push(`聊天里 ${r.chatDays} 天以前的图片、视频、文件`)
  return parts.join('，')
})
const bothVersions = computed(() => {
  const versions = new Set(report.value?.accounts.map((a) => a.version))
  return versions.has('3.x') && versions.has('4.x')
})

function show(r: WechatReport): void {
  report.value = r
  chosen.value = r.accounts.filter((a) => a.cacheFiles + a.chatFiles > 0).map((a) => a.id)
}

async function scan(): Promise<void> {
  if (loading.value || cleaning.value) return
  loading.value = true
  error.value = ''
  outcome.value = null
  try {
    show(await wechatScan(days.value))
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

// 换了天数：查过的话按新的天数重新数
watch(days, () => {
  if (report.value) void scan()
})

function resultText(r: WechatCleanResult): string {
  let text = `放进回收站了 ${r.files} 个文件，共 ${formatBytes(r.bytes)}。`
  if (r.failed) text += `有 ${r.failed} 个文件没放进去（可能正被别的程序用着），过一会儿可以再清一次。`
  if (r.cancelled) text += '你在 Windows 的提示框里点了「取消」，后面的没有再清。'
  if (r.partial) text += '文件太多，这一次没清完，再点一次「放进回收站」接着清。'
  if (r.files) {
    text += '先打开微信看看聊天记录和图片都正常，再清空回收站，这些地方才会空出来；清空以前，想要回来的都能在回收站里右键「还原」。'
  }
  return text
}

async function clean(): Promise<void> {
  if (cleaning.value) return
  cleaning.value = true
  let next: { ok: boolean; text: string }
  try {
    const r = await wechatClean(chosen.value, wantCache.value, wantChat.value, days.value)
    show(r.report)
    next = { ok: r.failed === 0 && !r.cancelled, text: resultText(r) }
  } catch (e) {
    next = { ok: false, text: errorText(e) }
  }
  cleaning.value = false
  // 先关确认框（焦点回到按钮上），再显示结果，焦点交给结果那句话
  confirming.value = false
  await nextTick()
  outcome.value = next
  await nextTick()
  message.value?.focus()
}
</script>

<template>
  <section class="card wechat-card" aria-labelledby="wechat-title">
    <component :is="props.level" id="wechat-title" :class="props.hideTitle ? 'visually-hidden' : 'section-title'">
      微信占的地方
    </component>
    <p class="muted small">
      微信的聊天图片、视频、文件和缓存默认都存在 C 盘，用久了能占几十 GB。这里找出这台电脑上微信 3.x、4.x 的每个账号，数一数缓存和很久以前的聊天文件各占多少，勾上的放进回收站。聊天记录（文字）不碰。
    </p>
    <p class="banner banner-warning small" role="note">
      清理前先退出微信。放进回收站以后，这些聊天里的图片、视频、文件在微信里就打不开了（缓存微信需要时一般会重新生成或下载）；清空回收站以前都能还原。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里是演示：列的是演示的账号，不会真的删。</p>
    <div class="row">
      <label for="wechat-days" class="small">聊天里的文件只算</label>
      <select id="wechat-days" v-model.number="days" class="input days" :disabled="loading || cleaning">
        <option v-for="d in DAY_OPTIONS" :key="d" :value="d">{{ d }} 天以前的</option>
      </select>
      <button type="button" class="btn btn-secondary btn-small" :disabled="loading || cleaning" @click="scan">
        {{ report ? '重新查' : '查一查' }}
      </button>
    </div>
    <p v-if="loading" class="loading-line small" role="status">
      <BusySpinner size="small" />正在数微信的文件，文件多的要等一会儿…
    </p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <template v-if="report && !loading">
      <p v-if="report.running" class="banner banner-warning small" role="alert">
        微信还开着：先在任务栏右下角的微信图标上点右键，选「退出微信」（有的版本叫「退出」），再点「重新查」。
      </p>
      <p v-if="!report.accounts.length" class="muted small">
        这台电脑上没找到微信的账号文件夹（找的是「文档」、用户文件夹和各个硬盘根目录下的 xwechat_files、WeChat Files，还有微信 3.x 记下的存储位置）。存储位置改到别处的，在微信的「设置 → 存储空间」（有的版本叫「文件管理」）里清理。
      </p>
      <ul v-else class="accounts">
        <li v-for="a in report.accounts" :key="a.id" class="account">
          <label class="check">
            <input v-model="chosen" type="checkbox" :value="a.id" :disabled="cleaning" />
            <span class="name">{{ accountName(a) }}</span>
          </label>
          <p class="muted small">缓存和临时文件（{{ report.cacheDays }} 天以前的）：{{ sizeText(a.cacheFiles, a.cacheBytes) }}</p>
          <p class="muted small">聊天里的图片、视频、文件（{{ report.chatDays }} 天以前的）：{{ sizeText(a.chatFiles, a.chatBytes) }}</p>
        </li>
      </ul>
      <p v-if="!report.complete" class="muted small">文件太多，没数完，上面是「至少这么多」。</p>
      <p v-if="bothVersions" class="muted small">
        新旧两版微信的文件夹都在：从 3.x 升级到 4.x 时，两边很多文件是同一份（硬链接），清掉一边不一定腾出地方；旧版留下的最好在新版微信的「设置 → 存储空间」里清。
      </p>
      <fieldset v-if="report.accounts.length" class="kinds">
        <legend class="small">清理哪些</legend>
        <label class="check">
          <input v-model="wantCache" type="checkbox" :disabled="cleaning" />
          <span class="small">缓存和临时文件（缩略图、表情、网页和小程序的缓存）</span>
        </label>
        <label class="check">
          <input v-model="wantChat" type="checkbox" :disabled="cleaning" />
          <span class="small">聊天里 {{ report.chatDays }} 天以前的图片、视频、文件（放进回收站以后在微信里就打不开了）</span>
        </label>
      </fieldset>
      <div v-if="report.accounts.length" class="row">
        <button
          type="button"
          class="btn btn-primary btn-small"
          :disabled="cleaning || report.running || total.files === 0"
          @click="confirming = true"
        >
          放进回收站
        </button>
        <span class="muted small">
          {{ total.files ? `一共 ${total.files} 个文件，${formatBytes(total.bytes)}` : '没有勾选要清理的文件' }}
        </span>
      </div>
    </template>
    <div v-if="outcome" :role="outcome.ok ? 'status' : 'alert'">
      <p ref="message" class="small outcome" :class="outcome.ok ? 'success-text' : 'danger-text'" tabindex="-1">
        {{ outcome.text }}
      </p>
    </div>
    <p class="muted small">企业微信、QQ 的请在它们自己的设置里清理：QQ 在「设置」里找「存储管理」。</p>

    <ConfirmDialog
      v-if="confirming"
      title="把这些文件放进回收站？"
      confirm-text="放进回收站"
      :busy="cleaning"
      @confirm="clean"
      @close="confirming = false"
    >
      <p>一共 {{ total.files }} 个文件，{{ formatBytes(total.bytes) }}：{{ kinds }}。</p>
      <p>和在资源管理器里按 Delete 一样，先放进回收站，清空回收站以前都能还原；回收站放不下的，Windows 会先问你要不要永久删除。</p>
      <p>聊天记录（文字）不会动；放进回收站以后，这些图片、视频、文件在微信里就打不开了。</p>
    </ConfirmDialog>
  </section>
</template>

<style scoped>
.wechat-card { display: flex; flex-direction: column; gap: 9px; }
.row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.days { width: auto; }
.accounts { display: flex; flex-direction: column; gap: 6px; padding: 0; list-style: none; }
.account { display: flex; flex-direction: column; gap: 2px; padding: 9px 12px; border-radius: var(--radius); background: var(--color-surface-2); }
.check { display: inline-flex; gap: 8px; align-items: center; min-width: 0; }
.name { font-weight: 600; }
.kinds { display: flex; flex-direction: column; gap: 6px; margin: 0; padding: 8px 12px; border: 1px solid var(--color-border); border-radius: var(--radius); }
.outcome { overflow-wrap: anywhere; }
.outcome:focus { outline: none; }
</style>
