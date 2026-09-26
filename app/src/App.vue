<script setup lang="ts">
import { computed, onMounted, useTemplateRef, watch } from 'vue'
import AppIcon from './components/AppIcon.vue'
import AppNav from './components/AppNav.vue'
import { loadCatalog, loadSystemInfo, nav, startupError, system, type PageId } from './state'
import HealthView from './views/HealthView.vue'
import JournalView from './views/JournalView.vue'
import ReportView from './views/ReportView.vue'
import SettingsView from './views/SettingsView.vue'
import SymptomsView from './views/SymptomsView.vue'
import ToolsView from './views/ToolsView.vue'

const views = {
  health: HealthView,
  symptoms: SymptomsView,
  settings: SettingsView,
  tools: ToolsView,
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

// 引擎没能启动时（startupError），各页都用不了，只显示原因和处理办法。
// 不给「再试一次」：后端只在启动时组装一次引擎，之后每个命令都返回同一个原因，重试不会有用。
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

        <section v-if="startupError" class="page startup-error" aria-labelledby="startup-error-title">
          <header class="page-header">
            <h1 id="startup-error-title" class="page-title" tabindex="-1">小药箱没能启动</h1>
            <p class="page-lead">小药箱自己的核心部分没有准备好，现在查不了也改不了；这台电脑没有被改动。</p>
          </header>

          <div class="banner banner-error" role="alert">
            <div>
              <p class="banner-title">原因</p>
              <p class="pre-text">{{ startupError }}</p>
            </div>
          </div>

          <div class="card startup-steps">
            <h2 class="section-title">可以这样试试</h2>
            <ol>
              <li>关掉小药箱，再重新打开。</li>
              <li>还不行的话，重启电脑，再打开小药箱。</li>
              <li>一直这样的话，把上面的原因拍下来或者抄下来，发给懂哥看看。</li>
            </ol>
          </div>
        </section>

        <!-- 换页时保留各页的状态（例如体检结果） -->
        <KeepAlive v-else>
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

.startup-steps {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.startup-steps ol {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 1.4em;
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
