<script setup lang="ts">
import type { ToolResult } from '../api/types'
import { toolStatusTone } from '../labels'
import AppIcon from './AppIcon.vue'
import ResultLinks from './ResultLinks.vue'

// 小工具跑完的结论：一句话（颜色和图标表示结果），下一步、出错信息，和结果里的跳转按钮。

defineProps<{ result: ToolResult }>()
const emit = defineEmits<{ preview: [featureId: string] }>()
</script>

<template>
  <div class="tool-outcome">
    <div class="outcome" :class="`tone-${toolStatusTone[result.status]}`" role="status">
      <AppIcon v-if="result.status === 'ok'" name="check" :size="20" class="outcome-icon" />
      <AppIcon v-else-if="result.status !== 'na'" name="warning" :size="20" class="outcome-icon" />
      <div class="outcome-body">
        <p class="outcome-message">{{ result.message }}</p>
        <p v-if="result.next" class="small">下一步：{{ result.next }}</p>
        <p v-if="result.error" class="small">出错信息：{{ result.error }}</p>
      </div>
    </div>
    <ResultLinks :links="result.links" @preview="(featureId) => emit('preview', featureId)" />
  </div>
</template>

<style scoped>
.tool-outcome {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.outcome {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 14px;
  border-radius: var(--radius);
}

.outcome-icon {
  margin-top: 2px;
}

.outcome-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  overflow-wrap: anywhere;
}

.outcome-message {
  font-weight: 600;
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

.tone-neutral {
  background: var(--tone-neutral-bg);
  color: var(--tone-neutral-text);
}
</style>
