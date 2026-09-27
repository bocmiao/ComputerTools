// 键盘测试用的键位：按 KeyboardEvent.code 认键（和输入法、大小写无关）。w 是宽度（一个普通键是 1）。
// 只画常见的 104 键布局；笔记本没有的键（小键盘、右 Win）按不到也正常。

export interface KeyCap {
  code: string
  label: string
  w?: number
}

/** 一行里的 null 是空出来的一格 */
export type KeyRow = (KeyCap | null)[]

const letters = (s: string): KeyCap[] => [...s].map((c) => ({ code: `Key${c}`, label: c }))

export const MAIN_ROWS: KeyRow[] = [
  [
    { code: 'Escape', label: 'Esc' }, null,
    ...[1, 2, 3, 4].map((n) => ({ code: `F${n}`, label: `F${n}` })), null,
    ...[5, 6, 7, 8].map((n) => ({ code: `F${n}`, label: `F${n}` })), null,
    ...[9, 10, 11, 12].map((n) => ({ code: `F${n}`, label: `F${n}` })),
  ],
  [
    { code: 'Backquote', label: '`' },
    ...[1, 2, 3, 4, 5, 6, 7, 8, 9, 0].map((n) => ({ code: `Digit${n}`, label: String(n) })),
    { code: 'Minus', label: '-' }, { code: 'Equal', label: '=' }, { code: 'Backspace', label: '退格', w: 2 },
  ],
  [
    { code: 'Tab', label: 'Tab', w: 1.5 }, ...letters('QWERTYUIOP'),
    { code: 'BracketLeft', label: '[' }, { code: 'BracketRight', label: ']' }, { code: 'Backslash', label: '\\', w: 1.5 },
  ],
  [
    { code: 'CapsLock', label: '大写锁定', w: 1.75 }, ...letters('ASDFGHJKL'),
    { code: 'Semicolon', label: ';' }, { code: 'Quote', label: "'" }, { code: 'Enter', label: '回车', w: 2.25 },
  ],
  [
    { code: 'ShiftLeft', label: 'Shift', w: 2.25 }, ...letters('ZXCVBNM'),
    { code: 'Comma', label: ',' }, { code: 'Period', label: '.' }, { code: 'Slash', label: '/' },
    { code: 'ShiftRight', label: 'Shift', w: 2.75 },
  ],
  [
    { code: 'ControlLeft', label: 'Ctrl', w: 1.25 }, { code: 'MetaLeft', label: 'Win', w: 1.25 },
    { code: 'AltLeft', label: 'Alt', w: 1.25 }, { code: 'Space', label: '空格', w: 6.25 },
    { code: 'AltRight', label: 'Alt', w: 1.25 }, { code: 'MetaRight', label: 'Win', w: 1.25 },
    { code: 'ContextMenu', label: '菜单', w: 1.25 }, { code: 'ControlRight', label: 'Ctrl', w: 1.25 },
  ],
]

export const NAV_ROWS: KeyRow[] = [
  [{ code: 'PrintScreen', label: 'PrtSc' }, { code: 'ScrollLock', label: 'ScrLk' }, { code: 'Pause', label: 'Pause' }],
  [{ code: 'Insert', label: 'Ins' }, { code: 'Home', label: 'Home' }, { code: 'PageUp', label: 'PgUp' }],
  [{ code: 'Delete', label: 'Del' }, { code: 'End', label: 'End' }, { code: 'PageDown', label: 'PgDn' }],
  [null, null, null],
  [null, { code: 'ArrowUp', label: '↑' }, null],
  [{ code: 'ArrowLeft', label: '←' }, { code: 'ArrowDown', label: '↓' }, { code: 'ArrowRight', label: '→' }],
]

export const NUMPAD_ROWS: KeyRow[] = [
  [null, null, null, null],
  [{ code: 'NumLock', label: 'Num' }, { code: 'NumpadDivide', label: '/' }, { code: 'NumpadMultiply', label: '*' }, { code: 'NumpadSubtract', label: '-' }],
  [{ code: 'Numpad7', label: '7' }, { code: 'Numpad8', label: '8' }, { code: 'Numpad9', label: '9' }, { code: 'NumpadAdd', label: '+' }],
  [{ code: 'Numpad4', label: '4' }, { code: 'Numpad5', label: '5' }, { code: 'Numpad6', label: '6' }, null],
  [{ code: 'Numpad1', label: '1' }, { code: 'Numpad2', label: '2' }, { code: 'Numpad3', label: '3' }, { code: 'NumpadEnter', label: '回车' }],
  [{ code: 'Numpad0', label: '0', w: 2 }, { code: 'NumpadDecimal', label: '.' }, null],
]

/** 所有画出来的键（算「按过了几个」用） */
export function allKeyCodes(): string[] {
  return [...MAIN_ROWS, ...NAV_ROWS, ...NUMPAD_ROWS].flat().flatMap((k) => (k ? [k.code] : []))
}
