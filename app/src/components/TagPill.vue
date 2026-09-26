<script setup lang="ts">
import type { Tone } from '../labels'

withDefaults(defineProps<{ tone?: Tone; dot?: boolean }>(), { tone: 'neutral', dot: false })
</script>

<template>
  <span class="tag" :class="`tone-${tone}`">
    <span v-if="dot" class="dot" aria-hidden="true"></span>
    <slot />
  </span>
</template>

<style scoped>
.tag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 1px 10px;
  border-radius: 999px;
  font-size: var(--text-small);
  font-weight: 600;
  line-height: 1.75;
  white-space: nowrap;
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
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

.tone-neutral {
  background: var(--tone-neutral-bg);
  color: var(--tone-neutral-text);
}
</style>
