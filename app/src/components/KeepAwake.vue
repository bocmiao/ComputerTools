<script setup lang="ts">
import { onActivated, onMounted, ref } from 'vue'
import { awakeGet, awakeSet } from '../api'
import { errorText } from '../utils/format'

// 别让电脑自己睡着：只在小药箱开着的时候有效，关掉开关或者退出小药箱就恢复原样，不改电源设置。
// 开关的状态以后端为准：每次回到这一页都重新读一次。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const on = ref(false)
const display = ref(true)
const busy = ref(false)
const error = ref('')

async function refresh(): Promise<void> {
  try {
    const s = await awakeGet()
    on.value = s.on
    if (s.on) display.value = s.display
  } catch (e) {
    error.value = errorText(e)
  }
}

async function apply(nextOn: boolean): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const s = await awakeSet(nextOn, display.value)
    on.value = s.on
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = false
  }
}

/** 开着的时候改「屏幕也亮着」，马上按新的设置重新打开 */
function onDisplayChange(): void {
  if (on.value) void apply(true)
}

onMounted(refresh)
onActivated(refresh)
</script>

<template>
  <section class="group" aria-labelledby="keep-awake-title">
    <div class="card awake-card">
      <h2 id="keep-awake-title" :class="hideTitle ? 'visually-hidden' : 'section-title'">别让电脑自己睡着</h2>
      <p class="muted small">
        下载大文件、上网课、传文件的时候，电脑没人碰也不会自己睡着。只在小药箱开着的时候有效：关掉这个开关或者退出小药箱，就恢复原来的样子，不改电源设置。合上笔记本盖子、按电源键照样会睡。
      </p>
      <label class="check small"><input v-model="display" type="checkbox" :disabled="busy" @change="onDisplayChange" /> 屏幕也一直亮着（看网课、直播时选这个）</label>
      <div class="row">
        <button type="button" class="btn btn-small" :class="on ? 'btn-secondary' : 'btn-primary'" :disabled="busy" @click="apply(!on)">
          {{ on ? '关掉，恢复原样' : '打开' }}
        </button>
        <span v-if="on" class="success-text small" role="status">开着：{{ display ? '电脑不会睡，屏幕也不会关' : '电脑不会睡，屏幕到时间照样会关' }}</span>
      </div>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    </div>
  </section>
</template>

<style scoped>
.group { display: flex; flex-direction: column; }
.awake-card { display: flex; flex-direction: column; gap: 8px; }
.row { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; }
.check { display: inline-flex; gap: 6px; align-items: center; }
</style>
