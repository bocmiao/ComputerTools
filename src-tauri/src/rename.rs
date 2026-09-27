//! 批量重命名：只处理用户在系统对话框里选定的文件夹里直属的普通文件（不进子文件夹，不跟着快捷方式、链接走）。
//!
//! 规则按顺序作用在不含扩展名的名字上：查找替换 → 换成「新名字 + 序号」→ 前面、后面加字；扩展名单独处理
//! （不变、改成小写、换成别的）。可以只处理某几种扩展名，按文件名或修改时间排序（序号跟着这个顺序）。
//!
//! 安全上的几条：
//! - 预览时记下文件夹里所有名字和每个文件的大小、修改时间，执行前重新扫一遍，完全一样才执行；
//! - 新名字不能是 Windows 不允许的名字，不能和不参与的文件、文件夹重名，彼此也不能重名；
//! - 名字互相占用（a→b、b→a，或者重新编号）时，先全部改成临时名，再改成新名字；
//! - 用不覆盖的方式改名（MoveFileW），中途失败就把已经改的全部改回去；
//! - 执行以后记下这一批，可以撤销（改回原名）；换了文件夹、又执行了一次以后，上一批就不能撤销了。
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

/// 一次最多处理的文件数
const MAX_FILES: usize = 500;

/// Windows 的 MoveFileW 不会覆盖已有的文件（和 std::fs::rename 不一样）。
#[cfg(windows)]
fn move_new(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileW;

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: 两个参数都是以 0 结尾的 UTF-16 字符串，调用期间一直有效
    if unsafe { MoveFileW(source.as_ptr(), target.as_ptr()) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn move_new(source: &Path, target: &Path) -> std::io::Result<()> {
    if target.exists() && !same_name(source, target) {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "target exists"));
    }
    fs::rename(source, target)
}

#[cfg(not(windows))]
fn same_name(a: &Path, b: &Path) -> bool {
    a.file_name().map(|n| n.to_string_lossy().to_lowercase())
        == b.file_name().map(|n| n.to_string_lossy().to_lowercase())
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Order {
    #[default]
    Name,
    Modified,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionRule {
    #[default]
    Keep,
    Lower,
    Set,
}

/// 界面传来的规则。字段都可以不写。
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct RenameRules {
    /// 只处理这些扩展名，用逗号、空格或分号隔开（「jpg, png」）；空的表示全部
    pub extensions: String,
    pub order: Order,
    /// 查找替换：只在不含扩展名的名字里找，区分大小写；查找为空时不替换
    pub find: String,
    pub replace: String,
    /// 整个名字换成「新名字 + 序号」
    pub numbering: bool,
    pub base: String,
    pub start: u32,
    /// 序号的位数，0 表示自动（至少 2 位，够放下最大的序号）
    pub digits: u8,
    pub prefix: String,
    pub suffix: String,
    pub extension: ExtensionRule,
    pub new_extension: String,
}

impl Default for RenameRules {
    fn default() -> Self {
        Self {
            extensions: String::new(),
            order: Order::Name,
            find: String::new(),
            replace: String::new(),
            numbering: false,
            base: String::new(),
            start: 1,
            digits: 0,
            prefix: String::new(),
            suffix: String::new(),
            extension: ExtensionRule::Keep,
            new_extension: String::new(),
        }
    }
}

#[derive(Default)]
pub struct RenameState {
    pub folder: Option<PathBuf>,
    pub preview: Option<Snapshot>,
    /// 上一次执行的那一批，撤销用
    pub last: Option<Applied>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Snapshot {
    rules: RenameRules,
    /// 文件夹里所有东西的名字（小写，排过序）：多了、少了一个都算变了
    names: Vec<String>,
    /// 文件夹里一共有几个普通文件（包括扩展名不对、不处理的）
    total: usize,
    files: Vec<FileSnapshot>,
}

#[derive(Clone, PartialEq, Debug)]
struct FileSnapshot {
    source: String,
    target: String,
    size: u64,
    modified: Option<u128>,
}

impl FileSnapshot {
    fn changed(&self) -> bool {
        self.source != self.target
    }
}

/// 执行过的一批：每个文件现在叫什么、原来叫什么，还有大小和修改时间（撤销前核对是不是还是那个文件）。
#[derive(Clone, Debug)]
pub struct Applied {
    folder: PathBuf,
    files: Vec<FileSnapshot>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenameEntry {
    pub source: String,
    pub target: String,
    pub changed: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreview {
    pub folder: String,
    pub entries: Vec<RenameEntry>,
    /// 名字会变的文件数
    pub changed: usize,
    /// 不在处理范围里（扩展名不对）的文件数
    pub skipped: usize,
}

const RESERVED: &[&str] = &["con", "prn", "aux", "nul"];

/// Windows 能不能用这个名字：不能是空的，不能有 <>:"/\|?* 和控制字符，不能以空格、句点结尾，
/// 不能是 CON、NUL、COM1 这类设备名（带不带扩展名都不行），不能超过 255 个字符（UTF-16）。
pub(crate) fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("新名字是空的。".into());
    }
    if name.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)) {
        return Err(format!("「{name}」里有 Windows 文件名不允许的字符（< > : \" / \\ | ? *）。"));
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err(format!("「{name}」以空格或句点结尾，Windows 不允许。"));
    }
    let device = name.split('.').next().unwrap_or("").trim_end().to_lowercase();
    let numbered = device.len() == 4
        && (device.starts_with("com") || device.starts_with("lpt"))
        && device.as_bytes()[3].is_ascii_digit()
        && device.as_bytes()[3] != b'0';
    if RESERVED.contains(&device.as_str()) || numbered {
        return Err(format!("「{name}」是 Windows 保留的设备名，不能用作文件名。"));
    }
    if name.encode_utf16().count() > 255 {
        return Err(format!("新名字太长了（超过 255 个字符）：{name}"));
    }
    Ok(())
}

/// 规则里用户写的字不能有 Windows 不允许的字符（查找的内容除外：它只用来找）。
fn check_rules(rules: &RenameRules) -> Result<(), String> {
    for (label, text) in [
        ("替换成", &rules.replace),
        ("新名字", &rules.base),
        ("前面加", &rules.prefix),
        ("后面加", &rules.suffix),
        ("新扩展名", &rules.new_extension),
    ] {
        if text.chars().count() > 120 {
            return Err(format!("「{label}」最多 120 个字。"));
        }
        if text.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)) {
            return Err(format!("「{label}」里有 Windows 文件名不允许的字符（< > : \" / \\ | ? *）。"));
        }
    }
    if rules.find.chars().count() > 120 {
        return Err("「查找」最多 120 个字。".into());
    }
    if rules.digits > 9 {
        return Err("序号最多 9 位。".into());
    }
    Ok(())
}

/// 「jpg, .PNG；webp」→ {"jpg", "png", "webp"}
fn wanted_extensions(text: &str) -> HashSet<String> {
    text.split(|c: char| c == ',' || c == '，' || c == ';' || c == '；' || c.is_whitespace())
        .map(|s| s.trim().trim_start_matches('.').to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn split_name(name: &str) -> (String, Option<String>) {
    let path = Path::new(name);
    match (path.file_stem().and_then(|s| s.to_str()), path.extension().and_then(|s| s.to_str())) {
        (Some(stem), Some(ext)) => (stem.to_string(), Some(ext.to_string())),
        _ => (name.to_string(), None),
    }
}

/// 按规则算出一个文件的新名字。`index` 是它在要处理的文件里排第几（从 0 开始），`width` 是序号位数。
fn target_name(rules: &RenameRules, source: &str, index: usize, width: usize) -> String {
    let (stem, ext) = split_name(source);
    let mut name = stem;
    if !rules.find.is_empty() {
        name = name.replace(&rules.find, &rules.replace);
    }
    if rules.numbering {
        let number = u64::from(rules.start) + index as u64;
        name = format!("{}{number:0width$}", rules.base);
    }
    name = format!("{}{name}{}", rules.prefix, rules.suffix);
    let ext = match rules.extension {
        ExtensionRule::Keep => ext,
        ExtensionRule::Lower => ext.map(|e| e.to_lowercase()),
        ExtensionRule::Set => {
            let e = rules.new_extension.trim().trim_start_matches('.').to_string();
            (!e.is_empty()).then_some(e)
        }
    };
    match ext {
        Some(e) => format!("{name}.{e}"),
        None => name,
    }
}

fn modified_nanos(meta: &fs::Metadata) -> Option<u128> {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos())
}

fn scan(folder: &Path, rules: &RenameRules) -> Result<Snapshot, String> {
    check_rules(rules)?;
    let meta = fs::symlink_metadata(folder).map_err(|e| format!("打不开选的文件夹：{e}"))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("选的位置已经不是普通文件夹了，请重新选择。".into());
    }
    let wanted = wanted_extensions(&rules.extensions);
    let mut names = Vec::new();
    let mut files = Vec::new();
    for item in fs::read_dir(folder).map_err(|e| format!("读不了文件夹：{e}"))? {
        let item = item.map_err(|e| format!("读文件列表出错：{e}"))?;
        let name = item.file_name().into_string().map_err(|_| "文件夹里有名字显示不出来的文件。".to_string())?;
        names.push(name.to_lowercase());
        let kind = item.file_type().map_err(|e| format!("读文件属性出错：{e}"))?;
        if !kind.is_file() || kind.is_symlink() {
            continue;
        }
        let meta = item.metadata().map_err(|e| format!("读文件属性出错：{e}"))?;
        if names.len() > 20_000 {
            return Err("文件夹里的东西太多了，请选一个小一点的文件夹。".into());
        }
        files.push((name, meta));
    }
    names.sort();
    let total = files.len();
    if !wanted.is_empty() {
        files.retain(|(name, _)| split_name(name).1.is_some_and(|e| wanted.contains(&e.to_lowercase())));
    }
    if files.is_empty() {
        return Err(if total == 0 {
            "文件夹里没有可以改名的文件。".into()
        } else {
            "文件夹里没有这几种扩展名的文件。".into()
        });
    }
    if files.len() > MAX_FILES {
        return Err(format!(
            "一次最多改 {MAX_FILES} 个文件，这里有 {} 个。可以只选几种扩展名，或者分几个文件夹来改。",
            files.len()
        ));
    }
    match rules.order {
        Order::Name => files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then(a.0.cmp(&b.0))),
        Order::Modified => files.sort_by(|a, b| {
            modified_nanos(&a.1).cmp(&modified_nanos(&b.1)).then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        }),
    }
    let last = u64::from(rules.start) + files.len() as u64 - 1;
    let width = if rules.digits == 0 { last.to_string().len().max(2) } else { usize::from(rules.digits) };

    let mut snapshot = Vec::with_capacity(files.len());
    for (index, (source, meta)) in files.into_iter().enumerate() {
        let target = target_name(rules, &source, index, width);
        snapshot.push(FileSnapshot { source, target, size: meta.len(), modified: modified_nanos(&meta) });
    }

    // 新名字：合法、彼此不重名、不和不参与改名的东西（没选中的文件、文件夹、名字不变的文件）重名
    let moving: HashSet<String> = snapshot.iter().filter(|f| f.changed()).map(|f| f.source.to_lowercase()).collect();
    let mut targets = HashSet::new();
    for f in &snapshot {
        check_name(&f.target)?;
        if !targets.insert(f.target.to_lowercase()) {
            return Err(format!("有两个文件会被改成同一个名字「{}」，请换个规则（比如加上序号）。", f.target));
        }
    }
    for f in snapshot.iter().filter(|f| f.changed()) {
        let key = f.target.to_lowercase();
        if key != f.source.to_lowercase() && names.binary_search(&key).is_ok() && !moving.contains(&key) {
            return Err(format!("新名字「{}」和文件夹里已有的文件或文件夹重名了，请换个规则。", f.target));
        }
    }
    Ok(Snapshot { rules: rules.clone(), names, total, files: snapshot })
}

pub fn preview(state: &mut RenameState, rules: &RenameRules) -> Result<RenamePreview, String> {
    state.preview = None;
    let folder = state.folder.as_ref().ok_or("请先选择文件夹。")?;
    let snapshot = scan(folder, rules)?;
    let result = RenamePreview {
        folder: folder.display().to_string(),
        entries: snapshot
            .files
            .iter()
            .map(|f| RenameEntry { source: f.source.clone(), target: f.target.clone(), changed: f.changed() })
            .collect(),
        changed: snapshot.files.iter().filter(|f| f.changed()).count(),
        skipped: snapshot.total.saturating_sub(snapshot.files.len()),
    };
    state.preview = Some(snapshot);
    Ok(result)
}

/// 按顺序改名；名字互相占用时先改成临时名。任何一步失败，把已经做了的倒着改回去。
/// `moves` 是（现在的名字，新名字）。
fn run_moves(folder: &Path, moves: &[(String, String)]) -> Result<(), String> {
    let sources: HashSet<String> = moves.iter().map(|(s, _)| s.to_lowercase()).collect();
    let overlapping =
        moves.iter().any(|(s, t)| t.to_lowercase() != s.to_lowercase() && sources.contains(&t.to_lowercase()));
    let mut steps: Vec<(String, String)> = Vec::new();
    if overlapping {
        let tag = format!("medkit-{}-{}", std::process::id(), UNIX_EPOCH.elapsed().map(|d| d.as_millis()).unwrap_or(0));
        let temps: Vec<String> = (0..moves.len()).map(|i| format!("~{tag}-{i}.tmp")).collect();
        if temps.iter().any(|t| folder.join(t).exists()) {
            return Err("临时文件名被占用了，请稍后再试。".into());
        }
        for ((source, _), temp) in moves.iter().zip(&temps) {
            steps.push((source.clone(), temp.clone()));
        }
        for ((_, target), temp) in moves.iter().zip(&temps) {
            steps.push((temp.clone(), target.clone()));
        }
    } else {
        steps.extend(moves.iter().cloned());
    }
    let mut done: Vec<&(String, String)> = Vec::new();
    for step in &steps {
        if let Err(error) = move_new(&folder.join(&step.0), &folder.join(&step.1)) {
            let mut rollback_failed = 0;
            for (from, to) in done.iter().rev() {
                if move_new(&folder.join(to), &folder.join(from)).is_err() {
                    rollback_failed += 1;
                }
            }
            let shown = moves.iter().find(|(s, _)| *s == step.0).map_or(step.0.as_str(), |(s, _)| s.as_str());
            return Err(format!(
                "改「{shown}」的名字时出错：{error}。{}",
                if rollback_failed == 0 {
                    "已经把改过的都改回去了。".to_string()
                } else {
                    format!("有 {rollback_failed} 个文件没能改回去，请打开文件夹看一看。")
                }
            ));
        }
        done.push(step);
    }
    Ok(())
}

pub fn apply(state: &mut RenameState) -> Result<usize, String> {
    let expected = state.preview.take().ok_or("请先预览一下改名的结果。")?;
    let folder = state.folder.clone().ok_or("请重新选择文件夹。")?;
    if scan(&folder, &expected.rules)? != expected {
        return Err("预览以后文件夹里的东西变了，请重新预览。".into());
    }
    let changed: Vec<FileSnapshot> = expected.files.into_iter().filter(FileSnapshot::changed).collect();
    if changed.is_empty() {
        return Err("按这些规则，没有文件的名字会变。".into());
    }
    let moves: Vec<(String, String)> = changed.iter().map(|f| (f.source.clone(), f.target.clone())).collect();
    run_moves(&folder, &moves)?;
    state.last = Some(Applied { folder, files: changed.clone() });
    Ok(changed.len())
}

/// 撤销前核对：这些文件还在、还是那几个（大小、修改时间没变），原来的名字没被别的文件占用。
fn check_undo(last: &Applied) -> Result<(), String> {
    let renamed: HashSet<String> = last.files.iter().map(|f| f.target.to_lowercase()).collect();
    for f in &last.files {
        let meta = match fs::symlink_metadata(last.folder.join(&f.target)) {
            Ok(m) if m.is_file() => m,
            _ => return Err(format!("找不到「{}」了，没法撤销。", f.target)),
        };
        if meta.len() != f.size || modified_nanos(&meta) != f.modified {
            return Err(format!("「{}」改名以后被改动过，为了不弄乱，没有撤销。", f.target));
        }
        if f.source.to_lowercase() != f.target.to_lowercase()
            && last.folder.join(&f.source).exists()
            && !renamed.contains(&f.source.to_lowercase())
        {
            return Err(format!("原来的名字「{}」已经被别的文件用了，没法撤销。", f.source));
        }
    }
    Ok(())
}

/// 撤销上一次执行的那一批（改回原名）。核对不过或者改名出错时，这一批还留着，可以处理好以后再撤销。
pub fn undo(state: &mut RenameState) -> Result<usize, String> {
    let last = state.last.as_ref().ok_or("没有可以撤销的重命名。")?;
    check_undo(last)?;
    let moves: Vec<(String, String)> = last.files.iter().map(|f| (f.target.clone(), f.source.clone())).collect();
    run_moves(&last.folder, &moves)?;
    state.last = None;
    state.preview = None;
    Ok(moves.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("medkit-rename-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn file(&self, name: &str, body: &[u8]) -> &Self {
            fs::write(self.0.join(name), body).unwrap();
            self
        }
        fn state(&self) -> RenameState {
            RenameState { folder: Some(self.0.clone()), ..Default::default() }
        }
        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> =
                fs::read_dir(&self.0).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
            v.sort();
            v
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn numbering(base: &str) -> RenameRules {
        RenameRules { numbering: true, base: base.into(), ..Default::default() }
    }

    #[test]
    fn numbering_keeps_extensions_and_can_be_undone() {
        let d = Dir::new("number");
        d.file("b.txt", b"b").file("a.jpg", b"a");
        let mut state = d.state();
        let plan = preview(&mut state, &numbering("照片_")).unwrap();
        assert_eq!(plan.entries[0].target, "照片_01.jpg");
        assert_eq!(plan.entries[1].target, "照片_02.txt");
        assert_eq!(plan.changed, 2);
        assert_eq!(apply(&mut state).unwrap(), 2);
        assert_eq!(fs::read(d.0.join("照片_01.jpg")).unwrap(), b"a");
        assert_eq!(undo(&mut state).unwrap(), 2);
        assert_eq!(d.names(), vec!["a.jpg", "b.txt"]);
        assert!(undo(&mut state).is_err(), "撤销过一次就没有了");
    }

    #[test]
    fn find_replace_prefix_suffix_and_extension_rules() {
        let d = Dir::new("rules");
        d.file("IMG_001.JPG", b"1").file("IMG_002.JPG", b"2").file("notes.txt", b"n");
        let mut state = d.state();
        let rules = RenameRules {
            extensions: "jpg".into(),
            find: "IMG_".into(),
            replace: "旅行-".into(),
            prefix: "2026 ".into(),
            suffix: "（原图）".into(),
            extension: ExtensionRule::Lower,
            ..Default::default()
        };
        let plan = preview(&mut state, &rules).unwrap();
        assert_eq!(plan.entries.len(), 2, "只处理 jpg");
        assert_eq!(plan.skipped, 1);
        assert_eq!(plan.entries[0].target, "2026 旅行-001（原图）.jpg");
        apply(&mut state).unwrap();
        assert_eq!(d.names(), vec!["2026 旅行-001（原图）.jpg", "2026 旅行-002（原图）.jpg", "notes.txt"]);

        let set = RenameRules {
            extension: ExtensionRule::Set,
            new_extension: ".md".into(),
            extensions: "txt".into(),
            ..Default::default()
        };
        let plan = preview(&mut state, &set).unwrap();
        assert_eq!(plan.entries[0].target, "notes.md");
    }

    #[test]
    fn swapped_or_renumbered_names_go_through_temporary_names() {
        let d = Dir::new("swap");
        d.file("01.jpg", b"first").file("02.jpg", b"second");
        let mut state = d.state();
        // 从 2 开始编号：01→02、02→03，02 被占着
        let rules = RenameRules { numbering: true, start: 2, ..Default::default() };
        let plan = preview(&mut state, &rules).unwrap();
        assert_eq!(plan.entries[0].target, "02.jpg");
        assert_eq!(plan.entries[1].target, "03.jpg");
        apply(&mut state).unwrap();
        assert_eq!(fs::read(d.0.join("02.jpg")).unwrap(), b"first");
        assert_eq!(fs::read(d.0.join("03.jpg")).unwrap(), b"second");
        undo(&mut state).unwrap();
        assert_eq!(fs::read(d.0.join("01.jpg")).unwrap(), b"first");
        assert_eq!(fs::read(d.0.join("02.jpg")).unwrap(), b"second");
        assert_eq!(d.names(), vec!["01.jpg", "02.jpg"], "临时文件不能留下");
    }

    #[test]
    fn conflicts_and_bad_names_are_refused() {
        let d = Dir::new("conflict");
        d.file("a.txt", b"a").file("b.txt", b"b").file("keep.log", b"k");
        fs::create_dir(d.0.join("x.txt")).unwrap();
        let mut state = d.state();
        // 两个文件改成同一个名字
        let same =
            RenameRules { find: "a".into(), replace: "b".into(), extensions: "txt".into(), ..Default::default() };
        assert!(preview(&mut state, &same).unwrap_err().contains("同一个名字"));
        // 和不参与的文件夹重名
        let folder =
            RenameRules { find: "a".into(), replace: "x".into(), extensions: "txt".into(), ..Default::default() };
        assert!(preview(&mut state, &folder).unwrap_err().contains("重名"));
        // 设备名、非法字符
        let device = RenameRules {
            extensions: "txt".into(),
            numbering: true,
            base: "con".into(),
            digits: 1,
            start: 0,
            ..Default::default()
        };
        assert!(preview(&mut state, &device).is_ok(), "con0、con1 不是设备名");
        let reserved =
            RenameRules { extensions: "log".into(), find: "keep".into(), replace: "NUL".into(), ..Default::default() };
        assert!(preview(&mut state, &reserved).unwrap_err().contains("设备名"));
        let bad = RenameRules { prefix: "a/b".into(), ..Default::default() };
        assert!(preview(&mut state, &bad).unwrap_err().contains("不允许"));
        assert_eq!(d.names(), vec!["a.txt", "b.txt", "keep.log", "x.txt"], "预览不改任何东西");
    }

    #[test]
    fn changes_after_preview_stop_the_rename() {
        let d = Dir::new("changed");
        d.file("a.txt", b"a");
        let mut state = d.state();
        preview(&mut state, &numbering("新_")).unwrap();
        d.file("new.txt", b"new");
        assert!(apply(&mut state).unwrap_err().contains("变了"));
        assert!(d.0.join("a.txt").exists());
    }

    #[test]
    fn undo_refuses_when_a_file_changed_or_its_old_name_is_taken() {
        let d = Dir::new("undo");
        d.file("a.txt", b"a");
        let mut state = d.state();
        preview(&mut state, &numbering("n")).unwrap();
        apply(&mut state).unwrap();
        d.file("a.txt", b"someone else");
        assert!(undo(&mut state).unwrap_err().contains("已经被别的文件用了"));
        fs::remove_file(d.0.join("a.txt")).unwrap();
        assert_eq!(undo(&mut state).unwrap(), 1, "挡住的撤销还能再试");
        assert_eq!(d.names(), vec!["a.txt"]);
    }

    #[test]
    fn unchanged_files_are_left_alone() {
        let d = Dir::new("unchanged");
        d.file("photo.jpg", b"p").file("IMG_1.jpg", b"i");
        let mut state = d.state();
        let rules = RenameRules { find: "IMG_".into(), replace: "photo_".into(), ..Default::default() };
        let plan = preview(&mut state, &rules).unwrap();
        assert_eq!(plan.changed, 1);
        assert_eq!(apply(&mut state).unwrap(), 1);
        assert_eq!(d.names(), vec!["photo.jpg", "photo_1.jpg"]);
        let nothing = RenameRules { find: "zzz".into(), replace: "y".into(), ..Default::default() };
        preview(&mut state, &nothing).unwrap();
        assert!(apply(&mut state).unwrap_err().contains("没有文件的名字会变"));
    }
}
