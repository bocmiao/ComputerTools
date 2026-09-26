import type { Directive } from 'vue'

// 打开的对话框按顺序压栈：只有最上面的那个响应 Esc 和 Tab。
// 有对话框开着时，把主界面设为 inert（不能点、不能用 Tab 移过去、读屏软件也跳过）。

const stack: symbol[] = []

function setAppInert(inert: boolean): void {
  const root = document.getElementById('app')
  if (!root) return
  if (inert) root.setAttribute('inert', '')
  else root.removeAttribute('inert')
}

export function pushDialog(): symbol {
  const token = Symbol('dialog')
  stack.push(token)
  setAppInert(true)
  return token
}

export function removeDialog(token: symbol): void {
  const i = stack.indexOf(token)
  if (i >= 0) stack.splice(i, 1)
  if (stack.length === 0) setAppInert(false)
}

export function isTopDialog(token: symbol): boolean {
  return stack[stack.length - 1] === token
}

const FOCUSABLE = [
  'button:not([disabled])',
  'a[href]',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

export function focusableIn(el: HTMLElement): HTMLElement[] {
  return Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (x) => !x.hasAttribute('inert') && x.getClientRects().length > 0,
  )
}

/**
 * 标记对话框打开后先聚焦的元素：<button v-autofocus>。
 * 没有标记时，聚焦对话框本身（读屏软件会先读标题）。
 *
 * 对话框开着时内容也会变（例如执行完换成结果页），刚才按的按钮被换掉以后焦点会丢到 body 上；
 * 这时新出现的 v-autofocus 元素把焦点接过来，用键盘的人不会迷路。
 */
export const vAutofocus: Directive<HTMLElement> = {
  mounted(el) {
    el.setAttribute('data-autofocus', '')
    queueMicrotask(() => {
      const dialog = el.closest('[role="dialog"]')
      if (!dialog || !el.isConnected) return
      const active = document.activeElement
      const focusInsideDialog = active !== null && active !== dialog && dialog.contains(active)
      if (!focusInsideDialog) el.focus()
    })
  },
}
