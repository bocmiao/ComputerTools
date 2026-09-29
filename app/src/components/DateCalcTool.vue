<script setup lang="ts">
import { computed, ref } from 'vue'
import { between, dayNumber, describeDate, formatDate, fromDayNumber, parseDate, shiftDate, type Unit } from '../utils/dateCalc'

// 日期计算：两个日期相差多久（天数、周数、几年几个月几天、中间有几个工作日），从某天往后或往前推几天、几个工作日是哪天。
// 工作日只按周一到周五算，法定节假日和调休没算进去（界面上写明），规则见 utils/dateCalc.ts。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const now = new Date()
const today = formatDate({ y: now.getFullYear(), m: now.getMonth() + 1, d: now.getDate() })

const inThirtyDays = shiftDate(today, 30, 'days')
const start = ref(today)
const end = ref(inThirtyDays === null ? today : formatDate(fromDayNumber(inThirtyDays)))
const diff = computed(() => between(start.value, end.value))

const base = ref(today)
const amount = ref('30')
const direction = ref<'later' | 'earlier'>('later')
const unit = ref<Unit>('days')
const shifted = computed(() => {
  const text = amount.value.trim()
  if (!/^\d{1,6}$/.test(text)) return null
  const count = Number(text) * (direction.value === 'later' ? 1 : -1)
  const n = shiftDate(base.value, count, unit.value)
  return n === null ? null : describeDate(n)
})

const startText = computed(() => {
  const d = parseDate(start.value)
  return d ? describeDate(dayNumber(d)) : ''
})
const endText = computed(() => {
  const d = parseDate(end.value)
  return d ? describeDate(dayNumber(d)) : ''
})

const ymd = computed(() => {
  const r = diff.value
  if (!r) return ''
  const parts: string[] = []
  if (r.years) parts.push(`${r.years} 年`)
  if (r.months) parts.push(`${r.months} 个月`)
  if (r.monthDays || parts.length === 0) parts.push(`${r.monthDays} 天`)
  return parts.join(' ')
})
</script>

<template>
  <article class="card date-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">日期计算</h3>
    <p class="muted small">算两个日期差几天、几个工作日（合同期限、年龄、倒计时），或者从某天往后推多少天、多少个工作日是哪天（「7 个工作日内办结」）。</p>

    <h4 class="sub-title">两个日期相差多久</h4>
    <div class="row">
      <label class="field">
        <span class="field-label">开始</span>
        <input v-model="start" class="input date-input" type="date" />
      </label>
      <label class="field">
        <span class="field-label">结束</span>
        <input v-model="end" class="input date-input" type="date" />
      </label>
    </div>
    <template v-if="diff">
      <p class="muted small">{{ startText }} → {{ endText }}</p>
      <p class="big" aria-live="polite">
        <template v-if="diff.days === 0">是同一天</template>
        <template v-else>{{ diff.days > 0 ? '相差' : '结束比开始早' }} {{ Math.abs(diff.days) }} 天</template>
      </p>
      <ul class="facts small">
        <li v-if="diff.days !== 0">首尾两天都算的话是 {{ diff.inclusive }} 天</li>
        <li v-if="Math.abs(diff.days) >= 7">合 {{ diff.weeks }} 周{{ diff.weekRest ? ` ${diff.weekRest} 天` : '' }}</li>
        <li v-if="Math.abs(diff.days) >= 28">合 {{ ymd }}</li>
        <li>首尾都算，周一到周五有 {{ diff.workdays }} 天（没算法定节假日和调休）</li>
      </ul>
    </template>
    <p v-else class="muted small">选好两个日期就能算。</p>

    <h4 class="sub-title">从某天往后、往前推</h4>
    <div class="row">
      <label class="field">
        <span class="field-label">从</span>
        <input v-model="base" class="input date-input" type="date" />
      </label>
      <label class="field">
        <span class="field-label">往</span>
        <select v-model="direction" class="input narrow">
          <option value="later">后</option>
          <option value="earlier">前</option>
        </select>
      </label>
      <label class="field">
        <span class="field-label">推</span>
        <input v-model="amount" class="input narrow" type="text" inputmode="numeric" maxlength="6" autocomplete="off" />
      </label>
      <label class="field">
        <span class="field-label">单位</span>
        <select v-model="unit" class="input">
          <option value="days">天</option>
          <option value="workdays">个工作日</option>
        </select>
      </label>
    </div>
    <p v-if="shifted" class="big" aria-live="polite">是 {{ shifted }}</p>
    <p v-else class="muted small">填一个整数。</p>
    <p v-if="unit === 'workdays'" class="muted small">
      工作日只按周一到周五算，不算当天；遇到法定节假日（春节、国庆这些）和调休上班的周末，要按当年国务院公布的放假安排自己加减。
    </p>
  </article>
</template>

<style scoped>
.date-card { display: flex; flex-direction: column; gap: 9px; }
.sub-title { font-size: var(--text-small); font-weight: 700; margin-top: 6px; }
.row { display: flex; flex-wrap: wrap; align-items: flex-start; gap: 10px; }
.field { display: flex; flex-direction: column; gap: 4px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { min-width: 0; height: 42px; padding: 0 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); color: inherit; }
.date-input { width: 11.5em; }
.narrow { width: 6em; }
.big { font-size: 1.15rem; font-weight: 600; }
.facts { display: flex; flex-direction: column; gap: 2px; padding-left: 1.2em; }
</style>
