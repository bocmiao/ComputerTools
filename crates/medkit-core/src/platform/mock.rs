//! 内存里的假系统，用于测试和在非 Windows 平台上开发界面。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use super::{OpenRequest, OsInfo, PResult, Platform, PlatformError, UserIdentity};
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
}

pub struct MockPlatform {
    state: Mutex<State>,
    pub interactive: Option<UserIdentity>,
    pub process: Option<UserIdentity>,
    pub admin: bool,
    pub os: OsInfo,
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

    fn open(&self, request: &OpenRequest) -> PResult<()> {
        let mut state = self.state.lock().unwrap();
        if let OpenRequest::Program { exe, .. } = request
            && state.missing_programs.contains(&exe.to_ascii_lowercase())
        {
            return Err(PlatformError::NotFound((*exe).to_owned()));
        }
        state.opened.push(*request);
        Ok(())
    }
}
