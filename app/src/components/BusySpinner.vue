<script setup lang="ts">
withDefaults(defineProps<{ size?: 'small' | 'medium' | 'large' }>(), { size: 'medium' })
</script>

<template>
  <span class="spinner" :class="size" aria-hidden="true"></span>
</template>

<style scoped>
.spinner {
  display: inline-block;
  flex: none;
  border-style: solid;
  border-color: var(--color-border-strong);
  border-top-color: var(--color-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.small {
  width: 16px;
  height: 16px;
  border-width: 2px;
}

.medium {
  width: 20px;
  height: 20px;
  border-width: 2.5px;
}

.large {
  width: 30px;
  height: 30px;
  border-width: 3px;
}

@media (prefers-reduced-motion: reduce) {
  .spinner {
    animation-duration: 2s;
  }
}
</style>
