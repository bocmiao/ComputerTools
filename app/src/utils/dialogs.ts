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

// ── 关掉对话框以后把焦点还回去 ──
//
// 打开对话框的那个按钮，关掉时常常已经不在了（例如修好以后「预览并设置」换成了别的按钮、
// 体检卡片挪到了「正常」那一组），或者请求期间被禁用了（焦点这时已经掉到 body 上）。
// 所以打开时除了记下焦点元素，还记下它外面的卡片、列表项；原来的元素不能用时退回到它们，
// 再不行就退回到页面主标题（各页的 h1 都有 tabindex="-1"）。

/** 可以退回去的容器：卡片、列表项、分组 */
const RETURN_TARGETS = 'li, article, section, .card'

let lastFocused: HTMLElement | null = null
let trackingFocus = false

/** 记下最近一次拿到焦点的元素。按钮在请求期间被禁用、焦点掉到 body 上时，用它来找回原来的位置 */
function trackFocus(): void {
  if (trackingFocus) return
  trackingFocus = true
  document.addEventListener('focusin', (e) => {
    if (e.target instanceof HTMLElement && e.target !== document.body) lastFocused = e.target
  })
}

if (typeof document !== 'undefined') trackFocus()

export interface SavedFocus {
  element: HTMLElement | null
  /** element 外面的卡片、列表项，由近到远 */
  containers: HTMLElement[]
}

export function rememberFocus(): SavedFocus {
  const active = document.activeElement
  let element: HTMLElement | null = null
  if (active instanceof HTMLElement && active !== document.body) element = active
  else if (lastFocused?.isConnected) element = lastFocused
  const containers: HTMLElement[] = []
  for (let p = element?.parentElement ?? null; p && p !== document.body; p = p.parentElement) {
    if (p.matches(RETURN_TARGETS)) containers.push(p)
  }
  return { element, containers }
}

function canTakeFocus(el: HTMLElement): boolean {
  if (!el.isConnected || el.closest('[inert]') || el.getClientRects().length === 0) return false
  return !(el as { disabled?: boolean }).disabled
}

export function restoreFocus(saved: SavedFocus): void {
  if (saved.element && canTakeFocus(saved.element)) {
    saved.element.focus()
    return
  }
  for (const box of saved.containers) {
    if (!canTakeFocus(box)) continue
    // 卡片本身不能聚焦：临时给它 tabindex="-1"（不进 Tab 顺序，只接住这一次焦点）
    if (!box.hasAttribute('tabindex')) box.setAttribute('tabindex', '-1')
    box.focus()
    return
  }
  document.querySelector<HTMLElement>('#app main h1[tabindex]')?.focus()
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
