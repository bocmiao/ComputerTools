import { reactive } from 'vue'
import { toolOpen } from '../api'
import type { ToolSummary } from '../api/types'
import { toolOpenedText } from '../labels'
import { errorText } from './format'

// 打开系统工具、「设置」里的页面：小工具页的卡片和检测结果里的 tool: 按钮都用这一套。

export interface ToolOpenState {
  busy: boolean
  /** null：还在打开 */
  ok: boolean | null
  text: string
}

export function useToolOpen() {
  /** 小工具 ID → 最近一次打开的结果 */
  const states = reactive<Record<string, ToolOpenState>>({})
  /** 只认最后一次点击的结果（reset 以后，之前发出去的请求一律作废） */
  const seqs = new Map<string, number>()
  let counter = 0

  async function open(tool: ToolSummary): Promise<void> {
    if (states[tool.id]?.busy) return
    const seq = ++counter
    seqs.set(tool.id, seq)
    states[tool.id] = { busy: true, ok: null, text: '正在打开…' }
    let next: ToolOpenState
    try {
      await toolOpen(tool.id)
      next = { busy: false, ok: true, text: toolOpenedText }
    } catch (e) {
      next = { busy: false, ok: false, text: errorText(e) }
    }
    if (seqs.get(tool.id) === seq) states[tool.id] = next
  }

  /** 清掉所有提示（例如离开页面时：「已经打开了」过一会儿就不是真的了） */
  function reset(): void {
    for (const id of Object.keys(states)) delete states[id]
    seqs.clear()
  }

  return { states, open, reset }
}
