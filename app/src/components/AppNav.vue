<script setup lang="ts">
import { computed } from 'vue'
import { isTauri } from '../api'
import { goTo, nav, system, systemError, type PageId } from '../state'
import AppIcon, { type IconName } from './AppIcon.vue'

const items: { id: PageId; label: string; icon: IconName }[] = [
  { id: 'health', label: '体检', icon: 'health' },
  { id: 'symptoms', label: '按症状修', icon: 'symptoms' },
  { id: 'settings', label: '常用设置', icon: 'settings' },
  { id: 'journal', label: '修改日志', icon: 'journal' },
  { id: 'report', label: '诊断报告', icon: 'report' },
]

const demo = !isTauri()

const osName = computed(() => system.value?.osCaption.replace(/^Microsoft\s+/i, '') ?? '')
/** osCaption 里通常已经带着版本类型，没带时再单独显示 */
const showEdition = computed(() => !!system.value?.edition && !osName.value.includes(system.value.edition))
</script>

<template>
  <aside class="nav">
    <div class="brand">
      <span class="brand-icon"><AppIcon name="logo" :size="22" /></span>
      <span class="brand-name">电脑小药箱</span>
    </div>

    <nav aria-label="主要功能">
      <ul class="items">
        <li v-for="item in items" :key="item.id">
          <button
            type="button"
            class="item"
            :class="{ current: nav.page === item.id }"
            :aria-current="nav.page === item.id ? 'page' : undefined"
            @click="goTo(item.id)"
          >
            <AppIcon :name="item.icon" />
            <span>{{ item.label }}</span>
          </button>
        </li>
      </ul>
    </nav>

    <div class="footer">
      <p v-if="demo" class="demo">演示模式：显示的是示例数据，不会改动这台电脑</p>

      <section v-if="system" class="sys" aria-label="系统信息">
        <p class="sys-os">{{ osName }}</p>
        <p class="muted small">
          版本号 {{ system.build }}<template v-if="showEdition"> · {{ system.edition }}</template>
        </p>
        <p class="small" :class="system.isAdmin ? 'admin-yes' : 'admin-no'">
          {{ system.isAdmin ? '已用管理员身份运行' : '没有用管理员身份运行，部分修复做不了' }}
        </p>
        <p class="muted small">小药箱 {{ system.appVersion }} · 数据 {{ system.catalogVersion }}</p>
      </section>
      <p v-else-if="systemError" class="muted small">没读出系统信息：{{ systemError }}</p>
      <p v-else class="muted small">正在读取系统信息…</p>
    </div>
  </aside>
</template>

<style scoped>
.nav {
  display: flex;
  flex-direction: column;
  gap: 16px;
  height: 100%;
  padding: 18px 12px 16px;
  background: var(--color-nav-bg);
  border-right: 1px solid var(--color-border);
  overflow-y: auto;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 10px 6px;
}

.brand-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: var(--radius);
  background: var(--color-primary);
  color: var(--color-primary-text);
}

.brand-name {
  font-size: var(--text-large);
  font-weight: 600;
}

.items {
  display: flex;
  flex-direction: column;
  gap: 4px;
  list-style: none;
}

.item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  min-height: 44px;
  padding: 8px 12px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  font-size: 15.5px;
  text-align: left;
  cursor: pointer;
}

.item:hover {
  background: var(--color-surface-2);
}

.item.current {
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
  font-weight: 600;
}

.footer {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: auto;
  padding: 0 6px;
}

.demo {
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--tone-info-bg);
  color: var(--tone-info-text);
  font-size: var(--text-small);
}

.sys {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
}

.sys-os {
  font-weight: 600;
}

.admin-yes {
  color: var(--color-success-text);
}

.admin-no {
  color: var(--tone-advice-text);
}
</style>
