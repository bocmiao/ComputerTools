<script setup lang="ts">
import { nextTick, onBeforeMount, onBeforeUnmount, onMounted, useId, useTemplateRef } from 'vue'
import {
  focusableIn,
  isTopDialog,
  pushDialog,
  rememberFocus,
  removeDialog,
  restoreFocus,
  type SavedFocus,
} from '../utils/dialogs'
import AppIcon from './AppIcon.vue'

// 通用对话框：按 Esc 或点遮罩关闭（busy 时不能关），Tab 只在对话框里循环，
// 关掉后焦点回到原来的位置；原来的元素不在了或被禁用时，退回到它所在的卡片，再不行退回到页面标题。
// 用法：父组件用 v-if 控制显示，收到 close 事件时把它去掉。

const props = withDefaults(
  defineProps<{
    title: string
    /** 正在执行时为 true：不能关闭 */
    busy?: boolean
    wide?: boolean
  }>(),
  { busy: false, wide: false },
)

const emit = defineEmits<{ close: [] }>()

const titleId = useId()
const panel = useTemplateRef<HTMLElement>('panel')
let token: symbol | null = null
let savedFocus: SavedFocus | null = null
let pressedOnBackdrop = false

function requestClose(): void {
  if (!props.busy) emit('close')
}

function onKeydown(e: KeyboardEvent): void {
  if (!token || !isTopDialog(token)) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    requestClose()
    return
  }
  if (e.key !== 'Tab' || !panel.value) return
  const items = focusableIn(panel.value)
  const first = items[0]
  const last = items[items.length - 1]
  if (!first || !last) {
    e.preventDefault()
    panel.value.focus()
    return
  }
  const active = document.activeElement
  if (!panel.value.contains(active)) {
    e.preventDefault()
    first.focus()
  } else if (e.shiftKey && (active === first || active === panel.value)) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && active === last) {
    e.preventDefault()
    first.focus()
  }
}

function onBackdropDown(e: MouseEvent): void {
  pressedOnBackdrop = e.target === e.currentTarget
}

function onBackdropClick(e: MouseEvent): void {
  // 只有按下和松开都在遮罩上才算点遮罩（避免在对话框里拖选文字时误关）
  if (pressedOnBackdrop && e.target === e.currentTarget) requestClose()
  pressedOnBackdrop = false
}

// 在对话框里的元素抢到焦点之前，先记下原来的焦点（和它外面的卡片），关掉后还回去
onBeforeMount(() => {
  savedFocus = rememberFocus()
})

onMounted(async () => {
  token = pushDialog()
  document.addEventListener('keydown', onKeydown)
  await nextTick()
  // 已经有元素（v-autofocus）拿到焦点了就不动
  if (panel.value && panel.value.contains(document.activeElement) && document.activeElement !== panel.value) return
  const target = panel.value?.querySelector<HTMLElement>('[data-autofocus]') ?? panel.value
  target?.focus()
})

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown)
  // 先解除主界面的 inert，焦点才能回去
  if (token) removeDialog(token)
  token = null
  if (savedFocus) restoreFocus(savedFocus)
  savedFocus = null
})
</script>

<template>
  <Teleport to="body">
    <div class="backdrop" @mousedown="onBackdropDown" @click="onBackdropClick">
      <div
        ref="panel"
        class="panel"
        :class="{ wide }"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        :aria-busy="busy"
        tabindex="-1"
      >
        <header class="header">
          <h2 :id="titleId" class="title">{{ title }}</h2>
          <button type="button" class="close btn btn-ghost" :disabled="busy" aria-label="关闭" @click="requestClose">
            <AppIcon name="close" />
          </button>
        </header>
        <div class="body">
          <slot />
        </div>
        <footer v-if="$slots.footer" class="footer">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: var(--color-backdrop);
}

.panel {
  display: flex;
  flex-direction: column;
  width: 600px;
  max-width: 100%;
  max-height: calc(100vh - 48px);
  background: var(--color-surface);
  color: var(--color-text);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-dialog);
}

.panel.wide {
  width: 780px;
}

.panel:focus-visible {
  outline-offset: -3px;
}

.header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 20px 10px 24px;
}

.title {
  font-size: 19px;
  padding-top: 4px;
}

.close {
  min-height: 34px;
  padding: 4px 8px;
}

.body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 6px 24px 20px;
  overflow-y: auto;
}

.footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  padding: 14px 24px 18px;
  border-top: 1px solid var(--color-border);
}
</style>
