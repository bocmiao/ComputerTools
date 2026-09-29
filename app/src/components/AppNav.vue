<script setup lang="ts">
import { computed } from 'vue'
import { isTauri } from '../api'
import { goHome, goTo, health, nav, system, systemError, type PageId } from '../state'
import AppIcon, { type IconName } from './AppIcon.vue'
import GlobalSearch from './GlobalSearch.vue'

// 左边的导航：品牌、搜索框（症状、设置、工具一个框搜）、六个页面、最下面一张电脑信息的小卡片。
// 体检查出要处理的项目时，「体检」旁边显示个数。在症状详情、单个工具页里再点一下当前这一页，回到那一页的首页。

const items: { id: PageId; label: string; icon: IconName }[] = [
  { id: 'health', label: '体检', icon: 'health' },
  { id: 'symptoms', label: '按症状修', icon: 'symptoms' },
  { id: 'settings', label: '常用设置', icon: 'settings' },
  { id: 'tools', label: '工具箱', icon: 'tools' },
  { id: 'journal', label: '修改日志', icon: 'journal' },
  { id: 'report', label: '诊断报告', icon: 'report' },
]

const demo = !isTauri()

// 只显示普通人看得懂的：系统名称（已经带着家庭版、专业版这类中文名）、版本号、有没有管理员权限、小药箱版本。
// edition（注册表里的英文 EditionID）和数据版本（一串哈希）不显示，报告里有。
const osName = computed(() => system.value?.osCaption.replace(/^Microsoft\s+/i, '') ?? '')

function badge(id: PageId): number | null {
  return id === 'health' && health.attention ? health.attention : null
}
</script>

<template>
  <aside class="nav">
    <div class="brand">
      <span class="brand-icon"><AppIcon name="logo" :size="22" /></span>
      <span class="brand-name">电脑小药箱</span>
    </div>

    <GlobalSearch />

    <nav aria-label="主要功能">
      <ul class="items">
        <li v-for="item in items" :key="item.id">
          <button
            type="button"
            class="item"
            :class="{ current: nav.page === item.id }"
            :aria-current="nav.page === item.id ? 'page' : undefined"
            @click="nav.page === item.id ? goHome(item.id) : goTo(item.id)"
          >
            <AppIcon :name="item.icon" />
            <span class="item-label">{{ item.label }}</span>
            <span v-if="badge(item.id)" class="badge" :aria-label="`${badge(item.id)} 项要处理`">{{ badge(item.id) }}</span>
          </button>
        </li>
      </ul>
    </nav>

    <div class="footer">
      <p v-if="demo" class="demo">演示模式：显示的是示例数据，不会改动这台电脑</p>

      <section v-if="system" class="sys" aria-label="这台电脑">
        <p class="sys-os">{{ osName }}</p>
        <p class="muted small">版本 {{ system.build }} · 小药箱 {{ system.appVersion }}</p>
        <p class="small admin" :class="system.isAdmin ? 'admin-yes' : 'admin-no'">
          <span class="admin-dot" aria-hidden="true"></span>
          {{ system.isAdmin ? '已用管理员身份运行' : '没有用管理员身份运行，部分修复做不了' }}
        </p>
      </section>
      <!-- 原因在右边的全局错误页里写着，这里只简单说一句 -->
      <p v-else-if="systemError" class="muted small">没读出系统信息</p>
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
  padding: 18px 14px 14px;
  background: var(--color-nav-bg);
  border-right: 1px solid var(--color-border);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 6px 2px;
}

.brand-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 9px;
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
  gap: 2px;
  list-style: none;
}

.item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  min-height: 42px;
  padding: 8px 12px;
  border: none;
  border-radius: 8px;
  background: transparent;
  font-size: 15px;
  text-align: left;
  cursor: pointer;
}

.item:hover {
  background: var(--color-surface-2);
}

.item.current {
  background: var(--color-surface);
  color: var(--color-primary-soft-text);
  font-weight: 600;
  box-shadow: 0 1px 2px rgb(16 24 40 / 0.08);
}

.item-label {
  flex: 1;
}

.badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  height: 20px;
  padding: 0 6px;
  border-radius: 10px;
  background: var(--tone-advice-bg);
  color: var(--tone-advice-text);
  font-size: 12px;
  font-weight: 700;
}

.footer {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: auto;
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
  padding: 12px 14px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
  background: var(--color-surface);
}

.sys-os {
  font-size: var(--text-small);
  font-weight: 600;
}

.admin {
  display: flex;
  align-items: center;
  gap: 6px;
}

.admin-dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}

.admin-yes {
  color: var(--color-success-text);
}

.admin-no {
  color: var(--tone-advice-text);
}
</style>
