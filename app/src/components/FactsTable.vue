<script setup lang="ts">
import { computed } from 'vue'
import { simpleFacts } from '../utils/format'

const props = defineProps<{ facts: Record<string, unknown> }>()

// 只显示简单值；嵌套的对象、数组不显示
const rows = computed(() => simpleFacts(props.facts))
</script>

<template>
  <div v-if="rows.length" class="table-wrap">
    <table class="table">
      <caption class="visually-hidden">详细数据</caption>
      <tbody>
        <tr v-for="row in rows" :key="row.key">
          <th scope="row" class="mono key">{{ row.key }}</th>
          <td>{{ row.value }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.key {
  width: 1%;
  white-space: nowrap;
  font-weight: 400;
  color: var(--color-text-muted);
}
</style>
