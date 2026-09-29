<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onDeactivated, reactive, ref } from 'vue'
import { isTauri, screenFullscreen, toolOpen } from '../api'
import { errorText } from '../utils/format'
import { MAIN_ROWS, NAV_ROWS, NUMPAD_ROWS, allKeyCodes, type KeyRow } from '../utils/keyboardLayout'

// 验机、找毛病用的几个小测试：屏幕坏点、键盘、鼠标、喇叭左右声道、麦克风、摄像头。
// 全在界面里做，不改电脑的任何设置。麦克风和摄像头第一次用时 WebView2 会弹出询问，点「允许」才用得了。
// 每一项的 id 是 device-test-<名字>（名字见 labels.ts 的 DEVICE_TEST_LABELS）：症状指引里的 test: 按钮跳到这里，
// 小工具页滚到那一项、把焦点放上去，highlight 是正在突出显示的那一项。
// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件。

defineProps<{ highlight?: string | null; hideTitle?: boolean }>()

// ── 屏幕坏点、漏光 ──
const SCREEN_COLORS = [
  { name: '黑色', value: '#000000' },
  { name: '白色', value: '#ffffff' },
  { name: '红色', value: '#ff0000' },
  { name: '绿色', value: '#00ff00' },
  { name: '蓝色', value: '#0000ff' },
  { name: '灰色', value: '#808080' },
]
const screenIndex = ref<number | null>(null)
const screenHint = ref(false)
const overlay = ref<HTMLElement | null>(null)
let hintTimer: number | undefined

// 小药箱窗口里用窗口自己的全屏（盖住任务栏）；浏览器里用网页的全屏。进不了全屏也照样铺满窗口
async function startScreen(): Promise<void> {
  screenIndex.value = 0
  screenHint.value = true
  clearTimeout(hintTimer)
  hintTimer = window.setTimeout(() => (screenHint.value = false), 3000)
  await nextTick()
  overlay.value?.focus()
  try {
    if (isTauri()) await screenFullscreen(true)
    else await overlay.value?.requestFullscreen()
  } catch {
    // 照样铺满窗口
  }
}

function stepColor(step: number): void {
  if (screenIndex.value === null) return
  const n = SCREEN_COLORS.length
  screenIndex.value = (screenIndex.value + step + n) % n
}

function stopScreen(): void {
  if (screenIndex.value === null) return
  screenIndex.value = null
  if (isTauri()) void screenFullscreen(false).catch(() => {})
  else if (document.fullscreenElement) void document.exitFullscreen().catch(() => {})
}

function onScreenKey(e: KeyboardEvent): void {
  e.preventDefault()
  if (e.key === 'Escape') stopScreen()
  else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') stepColor(-1)
  else stepColor(1)
}

// 按 Esc 退出全屏时浏览器先把全屏关了，这里跟着收起
function onFullscreenChange(): void {
  if (!document.fullscreenElement && screenIndex.value !== null) screenIndex.value = null
}
document.addEventListener('fullscreenchange', onFullscreenChange)

// ── 键盘 ──
const tested = reactive(new Set<string>())
const held = reactive(new Set<string>())
const lastKey = ref('')
const keyboardFocused = ref(false)
const totalKeys = allKeyCodes().length
const testedCount = computed(() => allKeyCodes().filter((c) => tested.has(c)).length)

let lastEscape = 0
const keyboardArea = ref<HTMLElement | null>(null)

function onKeyDown(e: KeyboardEvent): void {
  e.preventDefault()
  tested.add(e.code)
  held.add(e.code)
  lastKey.value = e.code
  // Tab 也要能测，所以用「连按两次 Esc」离开测试区（不然只用键盘的人出不去）
  if (e.code === 'Escape' && !e.repeat) {
    const now = performance.now()
    if (now - lastEscape < 800) keyboardArea.value?.blur()
    lastEscape = now
  }
}

function onKeyUp(e: KeyboardEvent): void {
  e.preventDefault()
  // 截屏键只有松开的时候才有
  tested.add(e.code)
  held.delete(e.code)
}

function resetKeyboard(): void {
  tested.clear()
  held.clear()
  lastKey.value = ''
}

function keyClass(code: string): Record<string, boolean> {
  return { held: held.has(code), tested: tested.has(code) }
}

const keyBlocks: { id: string; rows: KeyRow[] }[] = [
  { id: 'main', rows: MAIN_ROWS },
  { id: 'nav', rows: NAV_ROWS },
  { id: 'numpad', rows: NUMPAD_ROWS },
]

// ── 鼠标 ──
const BUTTONS = ['左键', '中键（滚轮按下）', '右键', '后退键', '前进键'] as const
const clicks = reactive<number[]>([0, 0, 0, 0, 0])
/** 两次按下隔得太近（90 毫秒以内），多半是按键老化、一下变两下 */
const bounces = reactive<number[]>([0, 0, 0, 0, 0])
const wheel = reactive({ up: 0, down: 0 })
const lastDown: number[] = [0, 0, 0, 0, 0]

function onMouseDown(e: MouseEvent): void {
  e.preventDefault()
  const b = e.button
  if (b < 0 || b > 4) return
  const now = performance.now()
  if (clicks[b]! > 0 && now - lastDown[b]! < 90) bounces[b]!++
  lastDown[b] = now
  clicks[b]!++
}

function onWheel(e: WheelEvent): void {
  e.preventDefault()
  if (e.deltaY < 0) wheel.up++
  else if (e.deltaY > 0) wheel.down++
}

function resetMouse(): void {
  for (let i = 0; i < 5; i++) {
    clicks[i] = 0
    bounces[i] = 0
    lastDown[i] = 0
  }
  wheel.up = 0
  wheel.down = 0
}

// ── 喇叭、麦克风、摄像头 ──
let audio: AudioContext | null = null
function audioContext(): AudioContext {
  audio ??= new AudioContext()
  return audio
}

const speakerError = ref('')
function beep(pan: number): void {
  speakerError.value = ''
  try {
    const ctx = audioContext()
    void ctx.resume()
    const osc = ctx.createOscillator()
    const gain = ctx.createGain()
    const panner = ctx.createStereoPanner()
    osc.frequency.value = 660
    panner.pan.value = pan
    const t = ctx.currentTime
    gain.gain.setValueAtTime(0, t)
    gain.gain.linearRampToValueAtTime(0.3, t + 0.05)
    gain.gain.setValueAtTime(0.3, t + 0.9)
    gain.gain.linearRampToValueAtTime(0, t + 1)
    osc.connect(gain).connect(panner).connect(ctx.destination)
    osc.start(t)
    osc.stop(t + 1.05)
  } catch (e) {
    speakerError.value = `放不出声音：${errorText(e)}`
  }
}

/** getUserMedia 的出错说成人话；设置页的小工具 ID 用来给「打开隐私设置」 */
function mediaError(e: unknown, device: '麦克风' | '摄像头'): string {
  const name = e instanceof DOMException ? e.name : ''
  const page = device === '麦克风' ? '麦克风' : '相机'
  if (name === 'NotAllowedError' || name === 'SecurityError') {
    return `没拿到${device}的使用许可：弹出询问时要点「允许」；没有弹出的话，到「设置 → 隐私和安全性 → ${page}」里打开「${page}访问权限」和「允许桌面应用访问你的${page}」。`
  }
  if (name === 'NotFoundError' || name === 'OverconstrainedError') return `没找到${device}：确认它插好了，笔记本的话看看有没有物理开关或者快捷键把它关掉了。`
  if (name === 'NotReadableError' || name === 'AbortError') return `${device}被别的软件占着（比如正在开会、直播的软件），先关掉那个软件再试。`
  return `打不开${device}：${errorText(e)}`
}

const micStream = ref<MediaStream | null>(null)
const micLevel = ref(0)
const micPeak = ref(0)
const micName = ref('')
const micError = ref('')
let micSource: MediaStreamAudioSourceNode | null = null
let micFrame = 0

async function startMic(): Promise<void> {
  micError.value = ''
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
    micStream.value = stream
    micName.value = stream.getAudioTracks()[0]?.label ?? ''
    const ctx = audioContext()
    void ctx.resume()
    micSource = ctx.createMediaStreamSource(stream)
    const analyser = ctx.createAnalyser()
    analyser.fftSize = 1024
    micSource.connect(analyser)
    const data = new Float32Array(analyser.fftSize)
    const tick = (): void => {
      analyser.getFloatTimeDomainData(data)
      let sum = 0
      for (const v of data) sum += v * v
      // 均方根换成 0～1 的音量，说话声大概在 0.3～0.7
      const level = Math.min(1, Math.sqrt(sum / data.length) * 4)
      micLevel.value = level
      micPeak.value = Math.max(micPeak.value * 0.995, level)
      micFrame = requestAnimationFrame(tick)
    }
    tick()
  } catch (e) {
    micError.value = mediaError(e, '麦克风')
  }
}

function stopMic(): void {
  cancelAnimationFrame(micFrame)
  micSource?.disconnect()
  micSource = null
  micStream.value?.getTracks().forEach((t) => t.stop())
  micStream.value = null
  micLevel.value = 0
  micPeak.value = 0
}

const camStream = ref<MediaStream | null>(null)
const camName = ref('')
const camError = ref('')
const video = ref<HTMLVideoElement | null>(null)

async function startCamera(): Promise<void> {
  camError.value = ''
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ video: true })
    camStream.value = stream
    camName.value = stream.getVideoTracks()[0]?.label ?? ''
    await nextTick()
    if (video.value) video.value.srcObject = stream
  } catch (e) {
    camError.value = mediaError(e, '摄像头')
  }
}

function stopCamera(): void {
  camStream.value?.getTracks().forEach((t) => t.stop())
  camStream.value = null
  if (video.value) video.value.srcObject = null
}

const openError = ref('')
async function openPrivacy(id: string): Promise<void> {
  openError.value = ''
  try {
    await toolOpen(id)
  } catch (e) {
    openError.value = errorText(e)
  }
}

// 切到别的页面时（这一页会留着），把麦克风、摄像头关掉，退出全屏
onDeactivated(() => {
  stopScreen()
  stopMic()
  stopCamera()
})

onBeforeUnmount(() => {
  document.removeEventListener('fullscreenchange', onFullscreenChange)
  stopScreen()
  clearTimeout(hintTimer)
  stopMic()
  stopCamera()
  void audio?.close()
})
</script>

<template>
  <section class="group" aria-labelledby="device-tests-title">
    <div class="group-head">
      <h2 id="device-tests-title" :class="hideTitle ? 'visually-hidden' : 'section-title'">屏幕、键盘、鼠标、声音测试</h2>
      <p class="muted small">买新电脑、二手电脑验机，或者怀疑哪里坏了的时候用。只是测试，不改电脑的任何设置。</p>
    </div>
    <div class="test-list">
      <article id="device-test-screen" class="card test-card" :class="{ highlight: highlight === 'screen' }" tabindex="-1">
        <h3 class="section-title">屏幕坏点和漏光</h3>
        <p class="muted small">整个屏幕依次显示黑、白、红、绿、蓝、灰，凑近看有没有不变色的小点（坏点、亮点）；黑色时在暗处看边缘有没有透光（漏光）。点鼠标或者按空格换颜色，按 Esc 退出。</p>
        <button type="button" class="btn btn-secondary btn-small self-start" @click="startScreen">开始（全屏）</button>
      </article>

      <article id="device-test-mouse" class="card test-card" :class="{ highlight: highlight === 'mouse' }" tabindex="-1">
        <h3 class="section-title">鼠标按键</h3>
        <p class="muted small">在下面的框里点各个按键、滚动滚轮。只按了一下却记了两下（「一下变两下」），多半是按键老化了。</p>
        <div
          class="mouse-area"
          role="application"
          aria-label="鼠标测试区：在这里点击、滚动"
          @mousedown="onMouseDown"
          @mouseup.prevent
          @auxclick.prevent
          @contextmenu.prevent
          @dblclick.prevent
          @wheel="onWheel"
        >
          在这里点击、滚动
        </div>
        <ul class="counts small" aria-live="polite">
          <li v-for="(name, i) in BUTTONS" :key="name">
            {{ name }}：{{ clicks[i] }} 次<span v-if="bounces[i]" class="danger-text">，其中 {{ bounces[i] }} 次疑似「一下变两下」</span>
          </li>
          <li>滚轮：向上 {{ wheel.up }}、向下 {{ wheel.down }}</li>
        </ul>
        <button type="button" class="btn btn-ghost btn-small self-start" @click="resetMouse">清零</button>
      </article>

      <article
        id="device-test-keyboard"
        class="card test-card wide"
        :class="{ highlight: highlight === 'keyboard' }"
        tabindex="-1"
      >
        <h3 class="section-title">键盘</h3>
        <p class="muted small">
          点一下下面的键盘，然后挨个按键：按过的键变成绿色，正按着的是深色。按了没变色的键可能坏了。Win 键会同时打开开始菜单；Fn 键和一些笔记本的功能键系统收不到，不会变色。
        </p>
        <div
          ref="keyboardArea"
          class="keyboard"
          :class="{ focused: keyboardFocused }"
          tabindex="0"
          role="application"
          aria-label="键盘测试区：点一下再按键"
          @keydown="onKeyDown"
          @keyup="onKeyUp"
          @focus="keyboardFocused = true"
          @blur="keyboardFocused = false; held.clear()"
        >
          <div v-for="block in keyBlocks" :key="block.id" class="kb-block" :class="block.id">
            <div v-for="(row, r) in block.rows" :key="r" class="kb-row">
              <template v-for="(k, i) in row" :key="i">
                <span v-if="k" class="kb-key" :class="keyClass(k.code)" :style="{ flexGrow: k.w ?? 1 }">{{ k.label }}</span>
                <span v-else class="kb-gap" />
              </template>
            </div>
          </div>
          <p v-if="!keyboardFocused" class="kb-hint">点这里，然后按键</p>
        </div>
        <p class="muted small" role="status">
          <template v-if="keyboardFocused">连按两次 Esc 离开测试区。</template>按过 {{ testedCount }} / {{ totalKeys }} 个键<template v-if="lastKey">，刚才按的是 {{ lastKey }}</template><template v-if="held.size > 1">，现在同时按着 {{ held.size }} 个</template>
        </p>
        <button type="button" class="btn btn-ghost btn-small self-start" @click="resetKeyboard">清空重来</button>
      </article>

      <article id="device-test-speaker" class="card test-card" :class="{ highlight: highlight === 'speaker' }" tabindex="-1">
        <h3 class="section-title">喇叭、耳机的左右声道</h3>
        <p class="muted small">点「左边响」只应该左边的喇叭（耳机左耳）响，「右边响」只应该右边响。反了说明左右接反了；有一边一直不响，可能是那一边坏了，或者声音设置里的「平衡」被调到了一边。</p>
        <div class="row">
          <button type="button" class="btn btn-secondary btn-small" @click="beep(-1)">左边响</button>
          <button type="button" class="btn btn-secondary btn-small" @click="beep(1)">右边响</button>
          <button type="button" class="btn btn-secondary btn-small" @click="beep(0)">两边一起响</button>
        </div>
        <p v-if="speakerError" class="danger-text small" role="alert">{{ speakerError }}</p>
      </article>

      <article
        id="device-test-microphone"
        class="card test-card"
        :class="{ highlight: highlight === 'microphone' }"
        tabindex="-1"
      >
        <h3 class="section-title">麦克风</h3>
        <p class="muted small">点「开始」后对着麦克风说话，下面的条会跟着跳。条一直不动，说明电脑没收到声音。第一次用时会弹出询问，点「允许」。</p>
        <div class="row">
          <button v-if="!micStream" type="button" class="btn btn-secondary btn-small" @click="startMic">开始</button>
          <button v-else type="button" class="btn btn-secondary btn-small" @click="stopMic">停止</button>
          <span v-if="micStream && micName" class="muted small">正在用：{{ micName }}</span>
        </div>
        <div v-if="micStream" class="meter" role="meter" aria-label="麦克风音量" :aria-valuenow="Math.round(micLevel * 100)" aria-valuemin="0" aria-valuemax="100">
          <span class="meter-bar" :style="{ width: `${micLevel * 100}%` }" />
          <span class="meter-peak" :style="{ left: `${micPeak * 100}%` }" />
        </div>
        <template v-if="micError">
          <p class="danger-text small" role="alert">{{ micError }}</p>
          <button type="button" class="btn btn-ghost btn-small self-start" @click="openPrivacy('settings.privacy-microphone')">打开麦克风的隐私设置</button>
        </template>
      </article>

      <article id="device-test-camera" class="card test-card" :class="{ highlight: highlight === 'camera' }" tabindex="-1">
        <h3 class="section-title">摄像头</h3>
        <p class="muted small">点「开始」后下面应该出现摄像头拍到的画面。第一次用时会弹出询问，点「允许」。画面只在这里显示，不保存、不上传。</p>
        <div class="row">
          <button v-if="!camStream" type="button" class="btn btn-secondary btn-small" @click="startCamera">开始</button>
          <button v-else type="button" class="btn btn-secondary btn-small" @click="stopCamera">停止</button>
          <span v-if="camStream && camName" class="muted small">正在用：{{ camName }}</span>
        </div>
        <video v-show="camStream" ref="video" class="camera" autoplay muted playsinline />
        <template v-if="camError">
          <p class="danger-text small" role="alert">{{ camError }}</p>
          <button type="button" class="btn btn-ghost btn-small self-start" @click="openPrivacy('settings.privacy-camera')">打开相机的隐私设置</button>
        </template>
      </article>
    </div>
    <p v-if="openError" class="danger-text small" role="alert">{{ openError }}</p>

    <div
      v-if="screenIndex !== null"
      ref="overlay"
      class="screen-overlay"
      tabindex="0"
      role="dialog"
      aria-label="屏幕坏点测试"
      :style="{ background: SCREEN_COLORS[screenIndex]!.value }"
      @click="stepColor(1)"
      @contextmenu.prevent="stepColor(-1)"
      @keydown="onScreenKey"
    >
      <p v-if="screenHint" class="screen-hint">{{ SCREEN_COLORS[screenIndex]!.name }} · 点鼠标或按空格换颜色，按 Esc 退出</p>
    </div>
  </section>
</template>

<style scoped>
.group, .group-head, .test-card { display: flex; flex-direction: column; }
.group { gap: 10px; }
.group-head { gap: 2px; }
.test-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; align-items: start; }
.test-card { gap: 9px; transition: border-color 0.2s, box-shadow 0.2s; }
.test-card.wide { grid-column: 1 / -1; }
.test-card.highlight { border-color: var(--color-primary); box-shadow: 0 0 0 3px var(--color-primary-soft); }
.row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.self-start { align-self: flex-start; }
.mouse-area {
  display: flex; align-items: center; justify-content: center; height: 110px; user-select: none;
  border: 2px dashed var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface-2);
  color: var(--color-text-muted);
}
.counts { display: flex; flex-direction: column; gap: 2px; padding-left: 1.2em; }
.keyboard {
  position: relative; display: flex; flex-wrap: wrap; gap: 12px; padding: 12px; user-select: none;
  border: 2px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface-2);
  outline: none; font-size: 12px;
}
.keyboard.focused { border-color: var(--color-primary); }
.kb-block { display: flex; flex-direction: column; gap: 4px; }
/* 三块按键的宽度比例：主键区 15 个键宽、中间 3 个、小键盘 4 个；窗口太窄时才换行 */
.kb-block.main { flex: 15 1 440px; min-width: 0; }
.kb-block.nav { flex: 3 1 90px; min-width: 0; }
.kb-block.numpad { flex: 4 1 120px; min-width: 0; }
.kb-row { display: flex; gap: 4px; }
.kb-key, .kb-gap { flex: 1 1 0; min-width: 0; height: 30px; }
.kb-key {
  display: flex; align-items: center; justify-content: center; overflow: hidden; white-space: nowrap;
  border: 1px solid var(--color-border-strong); border-radius: 4px; background: var(--color-surface);
}
.kb-block.nav .kb-key { font-size: 11px; letter-spacing: -0.02em; }
.kb-key.tested { background: var(--tone-ok-bg); color: var(--tone-ok-text); border-color: var(--tone-ok-dot); }
.kb-key.held { background: var(--tone-ok-dot); color: #fff; }
.kb-hint {
  position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; margin: 0;
  background: color-mix(in srgb, var(--color-surface) 70%, transparent); font-size: var(--text-large); font-weight: 600;
}
.meter { position: relative; height: 14px; border-radius: 7px; background: var(--color-surface-2); overflow: hidden; border: 1px solid var(--color-border-strong); }
.meter-bar { position: absolute; inset: 0 auto 0 0; background: var(--tone-ok-dot); transition: width 60ms linear; }
.meter-peak { position: absolute; top: 0; bottom: 0; width: 2px; background: var(--tone-advice-dot); }
.camera { width: 100%; max-height: 260px; border-radius: var(--radius); background: #000; object-fit: contain; }
.screen-overlay { position: fixed; inset: 0; z-index: 1000; cursor: none; outline: none; }
.screen-hint {
  position: absolute; left: 50%; bottom: 40px; transform: translateX(-50%); margin: 0; padding: 8px 16px;
  border-radius: 999px; background: rgba(0, 0, 0, 0.65); color: #fff; font-size: 16px; cursor: default;
}
@media (max-width: 900px) { .test-list { grid-template-columns: 1fr; } }
</style>
