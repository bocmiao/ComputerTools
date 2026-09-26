<script setup lang="ts">
import { computed } from 'vue'

// 显示注册表路径这类很长的位置：每个反斜杠后面都允许换行，免得从单词中间断开。
const props = defineProps<{ text: string }>()

const parts = computed(() => props.text.split('\\'))
</script>

<template>
  <span class="path"><template v-for="(part, i) in parts" :key="i">{{ part }}<template v-if="i < parts.length - 1">{{ '\\' }}<wbr /></template></template></span>
</template>

<style scoped>
.path {
  font-family: var(--font-mono);
  font-size: 0.92em;
  overflow-wrap: anywhere;
}
</style>
