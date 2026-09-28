//! 内存里的假系统，用于测试和在非 Windows 平台上开发界面。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use super::{
    FileStrings, FileUser, InstalledProgram, KeyboardAids, OpenRequest, OsInfo, PResult, Platform, PlatformError,
    PointedWindow, UserIdentity, WinsockEntry,
};
use crate::model::{Edition, StartType};
use crate::registry::{RegRoot, RegValue, key_ancestors};

type KeyId = (RegRoot, String);

#[derive(Default)]
struct State {
    /// 存在的键（大小写按 Windows 习惯不敏感，这里统一转小写存）
    keys: BTreeSet<KeyId>,
    values: BTreeMap<KeyId, BTreeMap<String, RegValue>>,
    services: BTreeMap<String, StartType>,
    /// 写这些「键\值」时模拟失败，用来测试回滚
    fail_writes: HashSet<String>,
    /// 这些「键\值」前几次写入成功，之后失败（值是还允许成功的次数）
    fail_writes_after: HashMap<String, usize>,
    /// 删这些键时模拟失败
    fail_key_deletes: HashSet<String>,
    /// 这些用户的注册表没有加载（已注销）
    unloaded_hives: HashSet<String>,
    /// 打开过的系统工具，按顺序
    opened: Vec<OpenRequest>,
    /// 打开这些程序时模拟「这台电脑上没有」
    missing_programs: HashSet<String>,
    /// 间接字符串（`@file,-id`）解出来的文字
    indirect: HashMap<String, String>,
    /// 在用这些文件的进程（路径统一转小写）
    file_users: Vec<(String, FileUser)>,
    /// 查这些文件时模拟失败
    fail_file_users: HashSet<String>,
    /// 鼠标指着的窗口
    pointed: Option<PointedWindow>,
    /// 程序文件的版本信息（路径统一转小写）
    file_strings: HashMap<String, FileStrings>,
    /// 「应用和功能」里的程序
    programs: Vec<InstalledProgram>,
    /// Winsock 目录；没设过时是 Windows 刚装好的样子（64 位、32 位各一项 TCP/IP）
    winsock: Option<Vec<WinsockEntry>>,
    /// 没有「获取帮助」应用
    no_get_help: bool,
    /// 桌面（资源管理器）没在运行，网页打不开
    no_desktop: bool,
}

pub struct MockPlatform {
    state: Mutex<State>,
    pub interactive: Option<UserIdentity>,
    pub process: Option<UserIdentity>,
    pub admin: bool,
    pub os: OsInfo,
    pub keyboard: KeyboardAids,
}

fn norm(key: &str) -> String {
    key.to_ascii_lowercase()
}

impl Default for MockPlatform {
    fn default() -> Self {
        let user = UserIdentity { sid: "S-1-5-21-1000-2000-3000-1001".into(), name: "MOCK-PC\\xiaoming".into() };
        Self {
            state: Mutex::default(),
            interactive: Some(user.clone()),
            process: Some(user),
            admin: true,
            os: OsInfo {
                caption: "Windows 11 家庭中文版（模拟）".into(),
                build: 26200,
                display_version: "25H2".into(),
                edition_id: "CoreCountrySpecific".into(),
                edition: Some(Edition::Home),
                computer_name: "MOCK-PC".into(),
            },
            keyboard: KeyboardAids::default(),
        }
    }
}

impl MockPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    /// 测试用：直接往假注册表里放一个值（会创建路径上的键）。
    pub fn seed_value(&self, root: &RegRoot, key: &str, name: &str, value: RegValue) {
        self.reg_set(root, key, name, &value).expect("seed");
    }

    pub fn seed_service(&self, name: &str, start_type: StartType) {
        self.state.lock().unwrap().services.insert(norm(name), start_type);
    }

    /// 测试用：让写某个值时失败。
    pub fn fail_write(&self, key: &str, name: &str) {
        self.state.lock().unwrap().fail_writes.insert(format!("{}\\{}", norm(key), name.to_ascii_lowercase()));
    }

    /// 测试用：让写某个值时，前 `successes` 次成功、之后失败（用来让回滚那一步失败）。
    pub fn fail_write_after(&self, key: &str, name: &str, successes: usize) {
        self.state
            .lock()
            .unwrap()
            .fail_writes_after
            .insert(format!("{}\\{}", norm(key), name.to_ascii_lowercase()), successes);
    }

    /// 测试用：让删某个键时失败。
    pub fn fail_key_delete(&self, key: &str) {
        self.state.lock().unwrap().fail_key_deletes.insert(norm(key));
    }

    /// 测试用：模拟某个用户已经注销，他的注册表没有加载。
    pub fn unload_hive(&self, sid: &str) {
        self.state.lock().unwrap().unloaded_hives.insert(sid.to_ascii_uppercase());
    }

    /// 测试用：模拟精简系统删掉了某个程序（文件名，不区分大小写）。
    pub fn remove_program(&self, exe: &str) {
        self.state.lock().unwrap().missing_programs.insert(exe.to_ascii_lowercase());
    }

    /// 测试用：这台电脑上没有「获取帮助」应用。
    pub fn remove_get_help(&self) {
        self.state.lock().unwrap().no_get_help = true;
    }

    /// 测试用：资源管理器没在运行，借它打开的网页打不开。
    pub fn stop_desktop(&self) {
        self.state.lock().unwrap().no_desktop = true;
    }

    /// 测试用：让 `indirect_string(source)` 返回 `text`。
    pub fn set_indirect(&self, source: &str, text: &str) {
        self.state.lock().unwrap().indirect.insert(source.to_owned(), text.to_owned());
    }

    /// 测试用：让 `user` 在用 `path` 这个文件。
    pub fn use_file(&self, path: &std::path::Path, user: FileUser) {
        self.state.lock().unwrap().file_users.push((norm(&path.to_string_lossy()), user));
    }

    /// 测试用：查 `path` 这个文件时失败。
    pub fn fail_file_users(&self, path: &std::path::Path) {
        self.state.lock().unwrap().fail_file_users.insert(norm(&path.to_string_lossy()));
    }

    /// 测试用：鼠标指着这个窗口（`None`：鼠标下面没有窗口）。
    pub fn point_at(&self, window: Option<PointedWindow>) {
        self.state.lock().unwrap().pointed = window;
    }

    /// 测试用：这个程序文件的版本信息。
    pub fn set_file_strings(&self, path: &std::path::Path, strings: FileStrings) {
        self.state.lock().unwrap().file_strings.insert(norm(&path.to_string_lossy()), strings);
    }

    /// 测试用：「应用和功能」里的程序。
    pub fn set_programs(&self, programs: Vec<InstalledProgram>) {
        self.state.lock().unwrap().programs = programs;
    }

    /// 测试用：Winsock 目录。
    pub fn set_winsock(&self, entries: Vec<WinsockEntry>) {
        self.state.lock().unwrap().winsock = Some(entries);
    }

    /// Windows 刚装好时的 Winsock 目录（简化成 64 位、32 位各一项 TCP/IP）。
    pub fn clean_winsock() -> Vec<WinsockEntry> {
        [false, true]
            .into_iter()
            .map(|wow64| WinsockEntry {
                protocol: "MSAFD Tcpip [TCP/IP]".into(),
                file: "mswsock.dll".into(),
                in_windows: true,
                exists: true,
                company: Some("Microsoft Corporation".into()),
                product: Some("Microsoft® Windows® Operating System".into()),
                chain_len: 1,
                family: crate::winsock::AF_INET,
                socket_type: crate::winsock::SOCK_STREAM,
                wow64,
            })
            .collect()
    }

    /// 测试用：打开过的系统工具。
    pub fn opened(&self) -> Vec<OpenRequest> {
        self.state.lock().unwrap().opened.clone()
    }

    pub fn key_exists(&self, root: &RegRoot, key: &str) -> bool {
        self.reg_key_exists(root, key).unwrap()
    }
}

impl Platform for MockPlatform {
    fn reg_get(&self, root: &RegRoot, key: &str, name: &str) -> PResult<Option<RegValue>> {
        let st = self.state.lock().unwrap();
        Ok(st.values.get(&(root.clone(), norm(key))).and_then(|m| m.get(&name.to_ascii_lowercase())).cloned())
    }

    fn reg_key_exists(&self, root: &RegRoot, key: &str) -> PResult<bool> {
        Ok(self.state.lock().unwrap().keys.contains(&(root.clone(), norm(key))))
    }

    fn reg_set(&self, root: &RegRoot, key: &str, name: &str, value: &RegValue) -> PResult<()> {
        let mut st = self.state.lock().unwrap();
        let id = format!("{}\\{}", norm(key), name.to_ascii_lowercase());
        if st.fail_writes.contains(&id) {
            return Err(PlatformError::AccessDenied(format!("{root}\\{key}\\{name}（模拟失败）")));
        }
        if let Some(left) = st.fail_writes_after.get_mut(&id) {
            if *left == 0 {
                return Err(PlatformError::AccessDenied(format!("{root}\\{key}\\{name}（模拟失败）")));
            }
            *left -= 1;
        }
        for k in key_ancestors(key) {
            st.keys.insert((root.clone(), norm(&k)));
        }
        st.values.entry((root.clone(), norm(key))).or_default().insert(name.to_ascii_lowercase(), value.clone());
        Ok(())
    }

    fn reg_delete_value(&self, root: &RegRoot, key: &str, name: &str) -> PResult<()> {
        let mut st = self.state.lock().unwrap();
        if let Some(m) = st.values.get_mut(&(root.clone(), norm(key))) {
            m.remove(&name.to_ascii_lowercase());
        }
        Ok(())
    }

    fn reg_delete_key_if_empty(&self, root: &RegRoot, key: &str) -> PResult<bool> {
        let mut st = self.state.lock().unwrap();
        if st.fail_key_deletes.contains(&norm(key)) {
            return Err(PlatformError::AccessDenied(format!("{root}\\{key}（模拟失败）")));
        }
        let id = (root.clone(), norm(key));
        if !st.keys.contains(&id) {
            return Ok(false);
        }
        let has_values = st.values.get(&id).is_some_and(|m| !m.is_empty());
        let prefix = format!("{}\\", norm(key));
        let has_children = st.keys.iter().any(|(r, k)| r == root && k.starts_with(&prefix));
        if has_values || has_children {
            return Ok(false);
        }
        st.keys.remove(&id);
        st.values.remove(&id);
        Ok(true)
    }

    fn user_hive_loaded(&self, sid: &str) -> bool {
        !self.state.lock().unwrap().unloaded_hives.contains(&sid.to_ascii_uppercase())
    }

    fn service_get(&self, name: &str) -> PResult<Option<StartType>> {
        Ok(self.state.lock().unwrap().services.get(&norm(name)).copied())
    }

    fn service_set(&self, name: &str, start_type: StartType) -> PResult<()> {
        let mut st = self.state.lock().unwrap();
        match st.services.get_mut(&norm(name)) {
            Some(slot) => {
                *slot = start_type;
                Ok(())
            }
            None => Err(PlatformError::NotFound(format!("服务 {name}"))),
        }
    }

    fn interactive_user(&self) -> Option<UserIdentity> {
        self.interactive.clone()
    }

    fn process_user(&self) -> Option<UserIdentity> {
        self.process.clone()
    }

    fn is_admin(&self) -> bool {
        self.admin
    }

    fn os_info(&self) -> OsInfo {
        self.os.clone()
    }

    fn keyboard_aids(&self) -> PResult<KeyboardAids> {
        Ok(self.keyboard)
    }

    fn open(&self, request: &OpenRequest) -> PResult<()> {
        let mut state = self.state.lock().unwrap();
        if let OpenRequest::Program { exe, .. } = request
            && state.missing_programs.contains(&exe.to_ascii_lowercase())
        {
            return Err(PlatformError::NotFound((*exe).to_owned()));
        }
        if matches!(request, OpenRequest::GetHelp(_)) && state.no_get_help {
            return Err(PlatformError::NotFound("获取帮助".into()));
        }
        if matches!(request, OpenRequest::Web(_)) && state.no_desktop {
            return Err(PlatformError::NotFound("资源管理器".into()));
        }
        state.opened.push(*request);
        Ok(())
    }

    fn indirect_string(&self, source: &str) -> Option<String> {
        self.state.lock().unwrap().indirect.get(source).cloned()
    }

    fn file_users(&self, files: &[std::path::PathBuf]) -> PResult<Vec<FileUser>> {
        let state = self.state.lock().unwrap();
        let wanted: Vec<String> = files.iter().map(|p| norm(&p.to_string_lossy())).collect();
        if let Some(bad) = wanted.iter().find(|p| state.fail_file_users.contains(*p)) {
            return Err(PlatformError::Other(format!("登记文件失败（模拟）：{bad}")));
        }
        let mut users: Vec<FileUser> = Vec::new();
        for (path, user) in &state.file_users {
            if wanted.contains(path) && !users.iter().any(|u| u.pid == user.pid && u.started == user.started) {
                users.push(user.clone());
            }
        }
        Ok(users)
    }

    fn pointed_window(&self) -> PResult<Option<PointedWindow>> {
        Ok(self.state.lock().unwrap().pointed.clone())
    }

    fn file_strings(&self, path: &std::path::Path) -> FileStrings {
        self.state.lock().unwrap().file_strings.get(&norm(&path.to_string_lossy())).cloned().unwrap_or_default()
    }

    fn installed_programs(&self) -> PResult<Vec<InstalledProgram>> {
        Ok(self.state.lock().unwrap().programs.clone())
    }

    fn winsock_catalog(&self) -> PResult<Vec<WinsockEntry>> {
        Ok(self.state.lock().unwrap().winsock.clone().unwrap_or_else(Self::clean_winsock))
    }
}
