<script setup lang="ts">
import { vAutofocus } from '../utils/dialogs'
import BusySpinner from './BusySpinner.vue'
import ModalDialog from './ModalDialog.vue'

// 简单的确认框。打开时焦点在「取消」上，免得误按回车。

withDefaults(
  defineProps<{
    title: string
    confirmText: string
    cancelText?: string
    busy?: boolean
    /** 内容比较多（例如要列出一串项目）时用宽一点的对话框 */
    wide?: boolean
  }>(),
  { cancelText: '取消', busy: false, wide: false },
)

const emit = defineEmits<{ confirm: []; close: [] }>()
</script>

<template>
  <ModalDialog :title="title" :busy="busy" :wide="wide" @close="emit('close')">
    <slot />
    <template #footer>
      <span v-if="busy" class="loading-line"><BusySpinner size="small" />正在处理…</span>
      <button type="button" class="btn btn-secondary" :disabled="busy" v-autofocus @click="emit('close')">
        {{ cancelText }}
      </button>
      <button type="button" class="btn btn-primary" :disabled="busy" @click="emit('confirm')">
        {{ confirmText }}
      </button>
    </template>
  </ModalDialog>
</template>
