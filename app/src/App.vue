<script setup lang="ts">
import { computed, onMounted, useTemplateRef, watch } from 'vue'
import AppIcon from './components/AppIcon.vue'
import AppNav from './components/AppNav.vue'
import { catalogError, loadCatalog, loadSystemInfo, nav, system, type PageId } from './state'
import HealthView from './views/HealthView.vue'
import JournalView from './views/JournalView.vue'
import ReportView from './views/ReportView.vue'
import SettingsView from './views/SettingsView.vue'
import SymptomsView from './views/SymptomsView.vue'

const views = {
  health: HealthView,
  symptoms: SymptomsView,
  settings: SettingsView,
  journal: JournalView,
  report: ReportView,
} satisfies Record<PageId, unknown>

const current = computed(() => views[nav.page])
const main = useTemplateRef<HTMLElement>('main')

// 换页时回到顶部
watch(
  () => nav.page,
  () => main.value?.scrollTo({ top: 0 }),
)

onMounted(() => {
  void loadSystemInfo()
  void loadCatalog()
})
</script>

<template>
  <div class="shell">
    <AppNav />
    <main ref="main" class="main">
      <div class="content">
        <div v-if="system?.elevatedUserMismatch" class="banner banner-warning mismatch" role="alert">
          <AppIcon name="warning" :size="22" />
          <div>
            <p class="banner-title">你是用别的管理员账户运行的，改动会写到登录用户身上</p>
            <p v-if="system.interactiveUser">
              小药箱会把设置改到正在用这台电脑的账户「{{ system.interactiveUser }}」上，而不是你输入密码的那个管理员账户。
            </p>
          </div>
        </div>

        <div v-if="catalogError" class="banner banner-error" role="alert">
          <div>
            <p class="banner-title">没能读出功能目录</p>
            <p>{{ catalogError }}</p>
            <button type="button" class="btn btn-secondary btn-small retry" @click="loadCatalog">重试</button>
          </div>
        </div>

        <!-- 换页时保留各页的状态（例如体检结果） -->
        <KeepAlive>
          <component :is="current" />
        </KeepAlive>
      </div>
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: 216px minmax(0, 1fr);
  height: 100%;
}

.main {
  min-width: 0;
  overflow-y: auto;
}

.content {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 900px;
  padding: 28px 36px 56px;
}

.mismatch {
  font-size: var(--text-base);
}

.retry {
  margin-top: 8px;
}

@media (max-width: 980px) {
  .shell {
    grid-template-columns: 188px minmax(0, 1fr);
  }

  .content {
    padding: 22px 22px 48px;
  }
}
</style>
