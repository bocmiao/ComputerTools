//! 由 Rust 实现的内置检测（`probe: { builtin: … }`）。
//! 名字要同时登记在 `catalog::BUILTIN_PROBES` 里。

use serde_json::{Value, json};

pub fn run(name: &str) -> Result<Value, String> {
    match name {
        "cpu-features" => Ok(cpu_features()),
        _ => Err(format!("不认识的内置检测：{name}")),
    }
}

/// Win11 24H2 起，CPU 必须支持 POPCNT 和 SSE4.2，否则即使绕过检查也装不上、开不了机。
fn cpu_features() -> Value {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let popcnt = std::arch::is_x86_feature_detected!("popcnt");
        let sse42 = std::arch::is_x86_feature_detected!("sse4.2");
        let result = if popcnt && sse42 { "ok" } else { "missing" };
        json!({ "result": result, "facts": { "popcnt": popcnt, "sse42": sse42 } })
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // ARM 电脑没有这两条 x86 指令的问题。
        json!({ "result": "ok", "facts": { "popcnt": true, "sse42": true, "arch": std::env::consts::ARCH } })
    }
}
