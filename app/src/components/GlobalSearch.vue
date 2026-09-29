<script setup lang="ts">
import { computed, ref, useId, useTemplateRef } from 'vue'
import { catalog, openLocalTool, openSetting, openSymptom, openTool } from '../state'
import { searchAll, type SearchHit, type SearchKind } from '../utils/search'
import AppIcon, { type IconName } from './AppIcon.vue'

// 导航栏上的搜索框：症状、常用设置、工具一个框搜。上下键挑、回车打开、Esc 关掉；Ctrl + K 跳到这里（App.vue）。

const query = ref('')
const open = ref(false)
const active = ref(0)
const input = useTemplateRef<HTMLInputElement>('input')
const listId = useId()

const hits = computed(() => searchAll(query.value, catalog.value, 8))
const showing = computed(() => open.value && query.value.trim() !== '')

const kindLabel: Record<SearchKind, string> = {
  symptom: '按症状修',
  setting: '常用设置',
  local: '工具箱',
  tool: '工具箱',
}
const kindIcon: Record<SearchKind, IconName> = {
  symptom: 'symptoms',
  setting: 'settings',
  local: 'tools',
  tool: 'tools',
}

function optionId(i: number): string {
  return `${listId}-${i}`
}

function choose(h: SearchHit): void {
  if (h.kind === 'symptom') openSymptom(h.id)
  else if (h.kind === 'setting') openSetting(h.id)
  else if (h.kind === 'local') openLocalTool(h.id)
  else openTool(h.id)
  query.value = ''
  open.value = false
  input.value?.blur()
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    if (query.value) query.value = ''
    else input.value?.blur()
    open.value = false
    return
  }
  if (!showing.value || hits.value.length === 0) return
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    active.value = (active.value + 1) % hits.value.length
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    active.value = (active.value - 1 + hits.value.length) % hits.value.length
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const h = hits.value[active.value]
    if (h) choose(h)
  }
}

function onInput(): void {
  open.value = true
  active.value = 0
}

/** 焦点离开搜索框和结果列表时收起来 */
function onFocusOut(e: FocusEvent): void {
  const next = e.relatedTarget as Node | null
  if (!next || !(e.currentTarget as HTMLElement).contains(next)) open.value = false
}
</script>

<template>
  <div class="gsearch" @focusout="onFocusOut">
    <label class="search-box nav-search">
      <AppIcon name="search" :size="16" />
      <input
        id="nav-search"
        ref="input"
        v-model="query"
        type="search"
        role="combobox"
        autocomplete="off"
        aria-label="搜索症状、设置、工具"
        aria-autocomplete="list"
        :aria-expanded="showing"
        :aria-controls="listId"
        :aria-activedescendant="showing && hits.length ? optionId(active) : undefined"
        placeholder="搜症状、设置、工具"
        @focus="open = true"
        @input="onInput"
        @keydown="onKeydown"
      />
      <kbd class="kbd" aria-hidden="true">Ctrl K</kbd>
    </label>
    <div v-show="showing" class="popover">
      <ul v-if="hits.length" :id="listId" role="listbox" aria-label="搜索结果" class="hits">
        <li
          v-for="(h, i) in hits"
          :id="optionId(i)"
          :key="`${h.kind}:${h.id}`"
          role="option"
          :aria-selected="i === active"
          class="hit"
          :class="{ active: i === active }"
          @mousedown.prevent="choose(h)"
          @mouseenter="active = i"
        >
          <AppIcon :name="kindIcon[h.kind]" :size="16" />
          <span class="hit-title">{{ h.title }}</span>
          <span class="hit-kind">{{ kindLabel[h.kind] }}</span>
        </li>
      </ul>
      <p v-else :id="listId" class="empty" role="status">没找到。换个说法试试，或者到「按症状修」里看看。</p>
    </div>
  </div>
</template>

<style scoped>
.gsearch {
  position: relative;
}

.nav-search {
  gap: 6px;
  min-height: 38px;
  padding: 0 8px 0 10px;
  border-color: var(--color-border);
}

.nav-search input {
  min-height: 36px;
  font-size: var(--text-small);
}

/* 不显示浏览器自带的「清空」小叉 */
.nav-search input::-webkit-search-cancel-button {
  display: none;
}

.kbd {
  flex: none;
  padding: 0 3px;
  border: 1px solid var(--color-border);
  border-radius: 4px;
  font-family: inherit;
  font-size: 10px;
  white-space: nowrap;
}

/* 窗口窄、导航栏变窄时，放不下「Ctrl K」的提示，只留搜索框 */
@media (max-width: 980px) {
  .kbd {
    display: none;
  }
}

.popover {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 40;
  width: 380px;
  max-width: calc(100vw - 32px);
  padding: 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
  background: var(--color-surface);
  box-shadow: var(--shadow-dialog);
}

.hits {
  display: flex;
  flex-direction: column;
  list-style: none;
}

.hit {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 38px;
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  color: var(--color-text-muted);
  cursor: pointer;
}

.hit.active {
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.hit-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--color-text);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.hit.active .hit-title {
  color: var(--color-primary-soft-text);
}

.hit-kind {
  flex: none;
  font-size: 12px;
}

.empty {
  padding: 8px 10px;
  color: var(--color-text-muted);
  font-size: var(--text-small);
}
</style>
