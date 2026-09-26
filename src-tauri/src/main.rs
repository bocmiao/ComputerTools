//! 电脑小药箱的桌面外壳（入口）。真正的逻辑在 lib.rs，这样 tests/ 里的集成测试也能用到。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    medkit_lib::run();
}
