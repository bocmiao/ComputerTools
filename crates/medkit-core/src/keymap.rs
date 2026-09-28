//! 键位重映射（改键）：Windows 自带的「扫描码映射」（微软《Keyboard and mouse class drivers》的
//! 「Scan code mapper for keyboards」）。映射存在 `HKLM\SYSTEM\CurrentControlSet\Control\Keyboard Layout` 的
//! `Scancode Map`（REG_BINARY，小端）里，重启以后生效，删掉这个值再重启就恢复原样：
//!
//! ```text
//! 0      4 字节  版本，全 0
//! 4      4 字节  标志，全 0
//! 8      4 字节  映射的条数（算上最后的结束标记，所以最少是 1）
//! 12     每条 4 字节：低 16 位是按下以后变成的扫描码，高 16 位是实际按下的键的扫描码；变成 0 就是这个键不起作用
//! 最后   4 字节  结束标记，全 0
//! ```
//!
//! 扫描码是 PS/2 第一套的按下码，带 E0 前缀的扩展键写成 0xE0xx（例如右 Ctrl 是 0xE01D）。
//! 能选的键写在代码里（[`KEYS`]）；这个值里有小药箱认不出来的键（别的改键软件设的）时，只允许整个清掉，不去改它。

use serde::{Deserialize, Serialize};

/// 修改日志里改键的「功能 ID」。
pub const FEATURE_ID: &str = "key-remap";
/// 映射所在的键（HKLM）。
pub const KEY: &str = r"HKLM\SYSTEM\CurrentControlSet\Control\Keyboard Layout";
/// 映射的值名。
pub const VALUE: &str = "Scancode Map";
/// 最多改几个键：一般用不了这么多，限制一下免得界面上误加一长串。
pub const MAX_MAPPINGS: usize = 24;

/// 能选的一个键：`id` 和浏览器 `KeyboardEvent.code` 的名字一样（界面上的键盘测试也用这套名字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyDef {
    pub id: &'static str,
    pub code: u16,
    pub label: &'static str,
    /// 只能当「变成」的目标（音量、播放这类多媒体键：很多键盘没有，但可以让别的键变成它）
    pub target_only: bool,
}

const fn key(id: &'static str, code: u16, label: &'static str) -> KeyDef {
    KeyDef { id, code, label, target_only: false }
}

const fn media(id: &'static str, code: u16, label: &'static str) -> KeyDef {
    KeyDef { id, code, label, target_only: true }
}

/// 能选的键，按界面上列出来的顺序。
pub const KEYS: &[KeyDef] = &[
    key("Escape", 0x01, "Esc"),
    key("F1", 0x3B, "F1"),
    key("F2", 0x3C, "F2"),
    key("F3", 0x3D, "F3"),
    key("F4", 0x3E, "F4"),
    key("F5", 0x3F, "F5"),
    key("F6", 0x40, "F6"),
    key("F7", 0x41, "F7"),
    key("F8", 0x42, "F8"),
    key("F9", 0x43, "F9"),
    key("F10", 0x44, "F10"),
    key("F11", 0x57, "F11"),
    key("F12", 0x58, "F12"),
    key("Backquote", 0x29, "` ~（数字 1 左边）"),
    key("Digit1", 0x02, "1"),
    key("Digit2", 0x03, "2"),
    key("Digit3", 0x04, "3"),
    key("Digit4", 0x05, "4"),
    key("Digit5", 0x06, "5"),
    key("Digit6", 0x07, "6"),
    key("Digit7", 0x08, "7"),
    key("Digit8", 0x09, "8"),
    key("Digit9", 0x0A, "9"),
    key("Digit0", 0x0B, "0"),
    key("Minus", 0x0C, "- _"),
    key("Equal", 0x0D, "= +"),
    key("Backspace", 0x0E, "Backspace（退格）"),
    key("Tab", 0x0F, "Tab"),
    key("KeyQ", 0x10, "Q"),
    key("KeyW", 0x11, "W"),
    key("KeyE", 0x12, "E"),
    key("KeyR", 0x13, "R"),
    key("KeyT", 0x14, "T"),
    key("KeyY", 0x15, "Y"),
    key("KeyU", 0x16, "U"),
    key("KeyI", 0x17, "I"),
    key("KeyO", 0x18, "O"),
    key("KeyP", 0x19, "P"),
    key("BracketLeft", 0x1A, "[ {"),
    key("BracketRight", 0x1B, "] }"),
    key("Backslash", 0x2B, "\\ |"),
    key("CapsLock", 0x3A, "Caps Lock（大写锁定）"),
    key("KeyA", 0x1E, "A"),
    key("KeyS", 0x1F, "S"),
    key("KeyD", 0x20, "D"),
    key("KeyF", 0x21, "F"),
    key("KeyG", 0x22, "G"),
    key("KeyH", 0x23, "H"),
    key("KeyJ", 0x24, "J"),
    key("KeyK", 0x25, "K"),
    key("KeyL", 0x26, "L"),
    key("Semicolon", 0x27, "; :"),
    key("Quote", 0x28, "' \""),
    key("Enter", 0x1C, "Enter（回车）"),
    key("ShiftLeft", 0x2A, "左 Shift"),
    key("KeyZ", 0x2C, "Z"),
    key("KeyX", 0x2D, "X"),
    key("KeyC", 0x2E, "C"),
    key("KeyV", 0x2F, "V"),
    key("KeyB", 0x30, "B"),
    key("KeyN", 0x31, "N"),
    key("KeyM", 0x32, "M"),
    key("Comma", 0x33, ", <"),
    key("Period", 0x34, ". >"),
    key("Slash", 0x35, "/ ?"),
    key("ShiftRight", 0x36, "右 Shift"),
    key("ControlLeft", 0x1D, "左 Ctrl"),
    key("MetaLeft", 0xE05B, "左 Win"),
    key("AltLeft", 0x38, "左 Alt"),
    key("Space", 0x39, "空格"),
    key("AltRight", 0xE038, "右 Alt"),
    key("MetaRight", 0xE05C, "右 Win"),
    key("ContextMenu", 0xE05D, "菜单键（右 Ctrl 左边）"),
    key("ControlRight", 0xE01D, "右 Ctrl"),
    key("PrintScreen", 0xE037, "Print Screen（截屏）"),
    key("ScrollLock", 0x46, "Scroll Lock"),
    key("Insert", 0xE052, "Insert（插入）"),
    key("Delete", 0xE053, "Delete（删除）"),
    key("Home", 0xE047, "Home"),
    key("End", 0xE04F, "End"),
    key("PageUp", 0xE049, "Page Up"),
    key("PageDown", 0xE051, "Page Down"),
    key("ArrowUp", 0xE048, "↑"),
    key("ArrowDown", 0xE050, "↓"),
    key("ArrowLeft", 0xE04B, "←"),
    key("ArrowRight", 0xE04D, "→"),
    key("NumLock", 0x45, "Num Lock（数字锁定）"),
    key("NumpadDivide", 0xE035, "小键盘 /"),
    key("NumpadMultiply", 0x37, "小键盘 *"),
    key("NumpadSubtract", 0x4A, "小键盘 -"),
    key("NumpadAdd", 0x4E, "小键盘 +"),
    key("NumpadEnter", 0xE01C, "小键盘回车"),
    key("Numpad7", 0x47, "小键盘 7"),
    key("Numpad8", 0x48, "小键盘 8"),
    key("Numpad9", 0x49, "小键盘 9"),
    key("Numpad4", 0x4B, "小键盘 4"),
    key("Numpad5", 0x4C, "小键盘 5"),
    key("Numpad6", 0x4D, "小键盘 6"),
    key("Numpad1", 0x4F, "小键盘 1"),
    key("Numpad2", 0x50, "小键盘 2"),
    key("Numpad3", 0x51, "小键盘 3"),
    key("Numpad0", 0x52, "小键盘 0"),
    key("NumpadDecimal", 0x53, "小键盘 ."),
    media("AudioVolumeMute", 0xE020, "静音"),
    media("AudioVolumeDown", 0xE02E, "音量减小"),
    media("AudioVolumeUp", 0xE030, "音量增大"),
    media("MediaPlayPause", 0xE022, "播放 / 暂停"),
    media("MediaTrackPrevious", 0xE010, "上一首"),
    media("MediaTrackNext", 0xE019, "下一首"),
];

pub fn by_id(id: &str) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.id == id)
}

pub fn by_code(code: u16) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.code == code)
}

/// 一条映射：按下 `from` 以后变成 `to`（`to` 为 0：这个键不起作用）。都是扫描码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping {
    pub from: u16,
    pub to: u16,
}

/// 解析 `Scancode Map` 的值。格式不对（长度、头、条数、结束标记）时返回 `None`。
pub fn parse(bytes: &[u8]) -> Option<Vec<Mapping>> {
    let dword =
        |at: usize| -> Option<u32> { bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) };
    if dword(0)? != 0 || dword(4)? != 0 {
        return None;
    }
    let count = usize::try_from(dword(8)?).ok()?;
    if count == 0 || bytes.len() != 12 + count.checked_mul(4)? {
        return None;
    }
    if dword(12 + (count - 1) * 4)? != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(count - 1);
    for i in 0..count - 1 {
        let v = dword(12 + i * 4)?;
        let (from, to) = ((v >> 16) as u16, (v & 0xFFFF) as u16);
        if from == 0 {
            // 没有按下的键：格式不对
            return None;
        }
        out.push(Mapping { from, to });
    }
    Some(out)
}

/// 生成 `Scancode Map` 的值。
pub fn build(mappings: &[Mapping]) -> Vec<u8> {
    let count = u32::try_from(mappings.len() + 1).expect("映射条数很少");
    let mut out = Vec::with_capacity(16 + mappings.len() * 4);
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&count.to_le_bytes());
    for m in mappings {
        out.extend_from_slice(&((u32::from(m.from) << 16) | u32::from(m.to)).to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]);
    out
}

/// 一条映射说成人话：「Caps Lock（大写锁定）→ 左 Ctrl」「右 Win → 不起作用」。认不出来的键写扫描码。
pub fn describe(m: Mapping) -> String {
    let name = |code: u16| by_code(code).map_or_else(|| format!("扫描码 {code:#06X}"), |k| k.label.to_owned());
    let to = if m.to == 0 { "不起作用".to_owned() } else { name(m.to) };
    format!("{} → {to}", name(m.from))
}

/// 整个值说成人话，修改日志里用。
pub fn describe_value(bytes: Option<&[u8]>) -> String {
    match bytes.map(parse) {
        None => "没有改键（Windows 默认）".to_owned(),
        Some(None) => "改键设置（小药箱认不出来的格式）".to_owned(),
        Some(Some(list)) if list.is_empty() => "没有改键（Windows 默认）".to_owned(),
        Some(Some(list)) => list.into_iter().map(describe).collect::<Vec<_>>().join("；"),
    }
}

/// 界面给的一条映射（键的名字，见 [`KEYS`]）。`to` 为空：这个键不起作用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingInput {
    pub from: String,
    #[serde(default)]
    pub to: Option<String>,
}

/// 把界面给的映射换成扫描码，顺便检查：键要在名单里、多媒体键只能当目标、同一个键不能改两次、不能改成自己、不能太多。
pub fn resolve(inputs: &[MappingInput]) -> Result<Vec<Mapping>, String> {
    if inputs.len() > MAX_MAPPINGS {
        return Err(format!("一次最多改 {MAX_MAPPINGS} 个键。"));
    }
    let mut out: Vec<Mapping> = Vec::with_capacity(inputs.len());
    for input in inputs {
        let from = by_id(&input.from).ok_or_else(|| format!("认不出这个键：{}", input.from))?;
        if from.target_only {
            return Err(format!("「{}」只能当「变成」的键，不能改它本身。", from.label));
        }
        let to = match &input.to {
            None => 0,
            Some(id) => by_id(id).ok_or_else(|| format!("认不出这个键：{id}"))?.code,
        };
        if to == from.code {
            return Err(format!("「{}」改成它自己，等于没改。", from.label));
        }
        if out.iter().any(|m| m.from == from.code) {
            return Err(format!("「{}」改了两次，只能留一个。", from.label));
        }
        out.push(Mapping { from: from.code, to });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.chunks(4).map(hex::encode).collect::<Vec<_>>().join(" ").to_uppercase()
    }

    #[test]
    fn microsoft_examples() {
        // 微软文档的例 1：左 Ctrl 和 Caps Lock 对换
        let swap = [Mapping { from: 0x1D, to: 0x3A }, Mapping { from: 0x3A, to: 0x1D }];
        assert_eq!(hex(&build(&swap)), "00000000 00000000 03000000 3A001D00 1D003A00 00000000");
        assert_eq!(parse(&build(&swap)).unwrap(), swap);
        // 例 2：去掉右 Ctrl，右 Alt 当静音键
        let bytes = hex::decode("000000000000000003000000".to_owned() + "00001DE0" + "20E038E0" + "00000000").unwrap();
        assert_eq!(parse(&bytes).unwrap(), vec![Mapping { from: 0xE01D, to: 0 }, Mapping { from: 0xE038, to: 0xE020 }]);
        assert_eq!(describe_value(Some(&bytes)), "右 Ctrl → 不起作用；右 Alt → 静音");
    }

    #[test]
    fn empty_map_and_bad_values() {
        assert_eq!(hex(&build(&[])), "00000000 00000000 01000000 00000000");
        assert_eq!(parse(&build(&[])).unwrap(), vec![]);
        assert_eq!(describe_value(None), "没有改键（Windows 默认）");
        // 头不是 0、条数对不上、没有结束标记、太短
        let mut bad = build(&[Mapping { from: 0x3A, to: 0x1D }]);
        bad[0] = 1;
        assert!(parse(&bad).is_none());
        let mut bad = build(&[Mapping { from: 0x3A, to: 0x1D }]);
        bad[8] = 3;
        assert!(parse(&bad).is_none());
        let mut bad = build(&[Mapping { from: 0x3A, to: 0x1D }]);
        let n = bad.len();
        bad[n - 1] = 1;
        assert!(parse(&bad).is_none());
        assert!(parse(&[0; 8]).is_none());
        assert!(parse(&hex::decode("00000000000000000000000000000000").unwrap()).is_none());
        assert_eq!(describe_value(Some(&[1, 2, 3])), "改键设置（小药箱认不出来的格式）");
    }

    #[test]
    fn keys_are_unique_and_resolvable() {
        let mut ids = std::collections::HashSet::new();
        let mut codes = std::collections::HashSet::new();
        for k in KEYS {
            assert!(ids.insert(k.id), "重复的名字 {}", k.id);
            assert!(codes.insert(k.code), "重复的扫描码 {:#06X}", k.code);
            assert!(k.code != 0);
        }
        let input = |from: &str, to: Option<&str>| MappingInput { from: from.into(), to: to.map(Into::into) };
        assert_eq!(
            resolve(&[input("CapsLock", Some("ControlLeft")), input("MetaLeft", None)]).unwrap(),
            vec![Mapping { from: 0x3A, to: 0x1D }, Mapping { from: 0xE05B, to: 0 }]
        );
        assert!(resolve(&[input("Nope", None)]).unwrap_err().contains("认不出"));
        assert!(resolve(&[input("AudioVolumeUp", None)]).unwrap_err().contains("只能当"));
        assert!(resolve(&[input("KeyA", Some("KeyA"))]).unwrap_err().contains("它自己"));
        assert!(resolve(&[input("KeyA", None), input("KeyA", Some("KeyB"))]).unwrap_err().contains("两次"));
        let many: Vec<MappingInput> = KEYS.iter().take(MAX_MAPPINGS + 1).map(|k| input(k.id, None)).collect();
        assert!(resolve(&many).unwrap_err().contains("最多"));
    }
}
