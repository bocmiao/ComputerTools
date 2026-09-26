<script setup lang="ts">
import { computed } from 'vue'
import type { Status } from '../api/types'
import BusySpinner from './BusySpinner.vue'

/** 症状检查里每一步的「灯」 */
export type LampState = 'pending' | 'running' | 'error' | Status

/** label：给读屏软件念的文字，不写时按 state 念（批量应用里的灯表示的是执行结果，要换说法） */
const props = defineProps<{ state: LampState; label?: string }>()

const text: Record<LampState, string> = {
  pending: '等待检查',
  running: '正在检查',
  error: '这一步出错了',
  ok: '正常',
  advice: '建议处理',
  manual: '需要人工',
  unknown: '没查出来',
  na: '不适用',
}

const toneClass = computed(() => {
  switch (props.state) {
    case 'ok':
      return 'lamp-ok'
    case 'advice':
      return 'lamp-advice'
    case 'manual':
      return 'lamp-manual'
    case 'unknown':
    case 'error':
      return 'lamp-unknown'
    case 'na':
      return 'lamp-na'
    default:
      return 'lamp-pending'
  }
})
</script>

<template>
  <span class="lamp-wrap">
    <BusySpinner v-if="state === 'running'" size="small" />
    <span v-else class="lamp" :class="toneClass" aria-hidden="true"></span>
    <span class="visually-hidden">{{ label ?? text[state] }}</span>
  </span>
</template>

<style scoped>
.lamp-wrap {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  flex: none;
}

.lamp {
  width: 14px;
  height: 14px;
  border-radius: 50%;
}

.lamp-pending {
  border: 2px solid var(--color-border-strong);
}

.lamp-ok {
  background: var(--tone-ok-dot);
  box-shadow: 0 0 0 4px var(--tone-ok-bg);
}

.lamp-advice {
  background: var(--tone-advice-dot);
  box-shadow: 0 0 0 4px var(--tone-advice-bg);
}

.lamp-manual {
  background: var(--tone-manual-dot);
  box-shadow: 0 0 0 4px var(--tone-manual-bg);
}

.lamp-unknown {
  background: var(--tone-unknown-dot);
  box-shadow: 0 0 0 4px var(--tone-unknown-bg);
}

.lamp-na {
  background: var(--tone-neutral-bg);
  border: 2px solid var(--tone-neutral-dot);
}
</style>
