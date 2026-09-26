# 功能编写指南

这份指南带你一步步写一个新的**检测**（查出问题）或**修复**（解决问题），再把它们挂到一个**症状**（用户说的「电脑怎么了」）上。字段的完整定义见[《架构与数据格式规范》](architecture.md)，这里只讲怎么做、以及容易踩的坑。

## 0. 动手之前

先回答四个问题，任何一个答不上来，先在 issue 里讨论：

1. **它在不在「不做」清单里？** 计划书[第五节](plan.md)列的事情一律不做，比如关 Defender、永久禁用更新、清注册表、删系统文件。CI 会拦截其中能自动识别的部分，剩下的靠审核。
2. **依据是什么？** 每个检测和修复都要写 `references`，优先引用微软官方文档。「网上都这么说」不算依据。
3. **谁来修？** 检测结果里的 `fixer` 要写清楚：小药箱能修（`medkit`）、用系统自带功能（`system`）、要用户自己动手（`user`）、找厂商（`vendor`）、找运营商（`isp`）、硬件问题（`hardware`）。修不了的问题也值得查出来，告诉用户该找谁，本身就是帮助。
4. **能不能撤销？** 修改系统的功能必须能撤销。实在不能撤销的（比如清理临时文件），写 `undo: none` 和 `irreversible_reason`，界面会在执行前明确告诉用户。

## 1. 写一个检测

一个检测由两部分组成：YAML 负责「说什么」，脚本负责「查什么」。

### 1.1 脚本：`scripts/checks/<分类>/<名字>.ps1`

```powershell
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$drive = Get-CimInstance -ClassName Win32_LogicalDisk -Filter "DeviceID='$($env:SystemDrive)'"
$freeGb = [math]::Round($drive.FreeSpace / 1GB, 1)
$freePct = [math]::Round(100 * $drive.FreeSpace / $drive.Size, 1)

$result = 'ok'
if ($freeGb -lt 5 -or $freePct -lt 5) { $result = 'critical' }
elseif ($freeGb -lt 15) { $result = 'low' }

[pscustomobject]@{
    result = $result
    facts  = @{ free_gb = $freeGb; free_pct = $freePct }
}
```

规则：

- 只输出**一个对象**，包含 `result`（结果代码）和可选的 `facts`（事实，给界面文字填空用）。
- **只写 ASCII**。界面上的文字一律写在 YAML 里，脚本里不出现中文。
- 必须能在 **Windows PowerShell 5.1** 上运行。下面这些 PowerShell 7 才有的写法不能用：`??`、`?:` 三元运算、`&&` / `||`、`ForEach-Object -Parallel`、`Get-Error`。
- 读结构化数据（注册表、CIM/WMI、事件日志），**不要解析命令输出的文字**：中文系统和英文系统的输出不一样。
- 原生命令的输出要接住：`$null = powercfg /a`，或者 `| Out-Null`。否则它们会混进结果里。
- 日期转成字符串再输出：`(Get-Date).ToString('o')`。
- 查不到、不适用的情况要有自己的结果代码（比如没有电池的台式机返回 `na`），不要让脚本报错。脚本报错时，界面会显示「这一项没查出来」。
- 需要读登录用户的注册表（`HKCU`）时，在 YAML 里写 `user_hive: true`，脚本加一个参数：

  ```powershell
  param([string]$UserHive = 'HKCU:')
  $key = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
  $proxy = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
  ```

  小药箱是以管理员身份运行的。如果用户用另一个管理员账户提权，`HKCU:` 指向的是那个管理员账户，而不是正在用电脑的人。`$UserHive` 总是指向登录用户，所以一定要用它。

### 1.2 YAML：`catalog/checks/<分类>/<名字>.yaml`

```yaml
id: disk.system-free-space
schema_version: 1
title: { zh-CN: 系统盘剩余空间 }
category: disk
probe: { script: checks/disk/system-free-space.ps1 }
results:
  ok:
    status: ok
    message: { zh-CN: "系统盘还剩 {free_gb} GB（{free_pct}%），空间充足。" }
  low:
    status: advice
    message: { zh-CN: "系统盘只剩 {free_gb} GB 了，建议清理。" }
    fixer: medkit
    links: [ "symptom:disk-full" ]
  critical:
    status: advice
    message: { zh-CN: "系统盘快满了（只剩 {free_gb} GB），可能导致更新失败、软件打不开。" }
    fixer: medkit
    next: { zh-CN: 先清理下载文件夹和回收站，再看看是不是有大文件放在桌面上。 }
    links: [ "symptom:disk-full" ]
references:
  - https://learn.microsoft.com/windows/win32/cimwin32prov/win32-logicaldisk
```

- `id` 一旦发布就**永不改名**（修改日志和用户报告里会引用它）。
- `results` 里要覆盖脚本可能返回的**每一个**结果代码。脚本返回了没定义的代码，界面会显示「没查出来」，CI 的冒烟测试也会失败。
- `status` 的取值：`ok`（正常）、`advice`（建议处理）、`manual`（需要人工）、`unknown`（查不出来）、`na`（不适用，不显示）。
- 文字里的 `{事实名}` 会被替换成 `facts` 里的值；小数保留一位，布尔值显示成「是 / 否」，数组用顿号连接。
- 写给普通人看：说「系统盘快满了」，不说「C: 可用空间低于阈值」。

## 2. 写一个修复

### 2.1 原语类：只改注册表值或服务启动类型

大多数设置类修复都可以只写 YAML。引擎会自动记录原值、核对结果、支持撤销，不需要写脚本。

```yaml
id: explorer.show-extensions
schema_version: 1
title: { zh-CN: 显示文件扩展名 }
description: { zh-CN: 在资源管理器里显示 .docx、.exe 这类扩展名，更容易认出伪装成文档的病毒。 }
category: settings
risk: safe
level: light
recommend: recommended
reboot: explorer
target: current-user
actions:
  - registry:
      key: HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced
      name: HideFileExt
      type: dword
      value: 0
windows_default:
  - registry:
      key: HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced
      name: HideFileExt
      type: dword
      value: 1
undo: auto
references:
  - https://support.microsoft.com/windows/common-file-name-extensions-in-windows-da4a4430-8e76-89c5-59f7-1cdbbc75cb01
```

- `target: current-user` 只能写 `HKCU\`；`target: machine` 只能写 `HKLM\` 或者服务。
- `windows_default` 是系统出厂时的状态，「恢复默认」和自动测试都会用到它。
- 要修的状态不是系统默认状态时（比如「代理开着、指向没人监听的端口」），再写一个 `break_actions` 描述故障状态，测试就用它来制造故障。
- `risk` 是 `caution` 或 `danger` 的修复，执行前会自动建还原点。
- 数据里以 `{` 开头的字符串（比如 CLSID）**必须加引号**：`name: "{20D04FE0-3AEA-1069-A2D8-08002B30309D}"`。

### 2.2 脚本类：原语做不到的事

需要调用系统命令、改文件、或者要先判断再决定怎么改的，写四个脚本：

| 字段 | 作用 | 返回 |
| --- | --- | --- |
| `detect` | 现在是不是已经修好了 | `{ state: applied / not-applied / partial / unknown, facts }` |
| `run` | 执行修复。**改之前先记下原状态** | `{ before, after, facts }` |
| `undo` | 撤销。参数 `-Before` 是 `run` 当初返回的 `before`（JSON 字符串） | 任意 |
| `break` | 只在测试时使用，制造出需要修复的状态 | 任意 |

```powershell
# scripts/features/power/hibernation-off.ps1
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'

$key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Power'
$enabled = (Get-ItemProperty -LiteralPath $key -Name HibernateEnabled -ErrorAction SilentlyContinue).HibernateEnabled
$null = powercfg.exe /hibernate off
if ($LASTEXITCODE -ne 0) { throw "powercfg exited with $LASTEXITCODE" }
[pscustomobject]@{ before = @{ hibernate_enabled = ($enabled -eq 1) }; after = @{ hibernate_enabled = $false } }
```

```powershell
# scripts/features/power/hibernation-restore.ps1
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Before)
$ErrorActionPreference = 'Stop'

$state = $Before | ConvertFrom-Json
if ($state.hibernate_enabled) {
    $null = powercfg.exe /hibernate on
    if ($LASTEXITCODE -ne 0) { throw "powercfg exited with $LASTEXITCODE" }
}
[pscustomobject]@{ restored = [bool]$state.hibernate_enabled }
```

- 失败时 `throw`，不要吞掉错误。引擎会把错误写进修改日志，界面会告诉用户「没有改成功」。
- **改到一半出错，要先把已经改了的退回去再 `throw`**（用 `try` / `catch`）。脚本出错时引擎拿不到 `before`，没法替你恢复。参考 `scripts/features/network/proxy-off-run.ps1`。
- 原生命令要检查 `$LASTEXITCODE`。

## 3. 挂到症状上：`catalog/symptoms/<名字>.yaml`

症状是用户的语言，比如「C 盘红了」「网页打不开但微信能用」。它把检测和修复串成一条排查路线：

```yaml
id: disk-full
schema_version: 1
title: { zh-CN: C 盘满了 }
summary: { zh-CN: C 盘变红、提示空间不足、更新装不上。 }
keywords: [ C盘红了, C盘满了, 空间不足, 磁盘已满 ]
causes:
  - { zh-CN: 下载文件夹和桌面上堆了大文件 }
  - { zh-CN: 微信、QQ 的聊天文件默认存在 C 盘 }
maturity: one-click
steps:
  - check: disk.system-free-space
    stop_on: [ ok ]
    fixes: [ power.hibernation-off ]
```

- `keywords` 写用户会怎么说，搜索主要靠它。多问问身边不懂电脑的人。
- `maturity`：`one-click`（能一键修）、`semi`（部分要用户动手）、`guide`（只有图文指引，这时必须写 `guide`）。
- `stop_on`：这一步的结论在列表里时，就不再往下查。

## 4. 在本地验证

```sh
cargo run -p medkit-data -- check     # 格式、引用、黑名单、脚本检查
cargo test                            # 引擎测试（任何系统都能跑）
```

用 VS Code 打开仓库，装上推荐的 YAML 插件，写 YAML 时就有字段补全和实时校验（Schema 在 `schema/` 下）。改了 `crates/medkit-core/src/model.rs` 里的类型以后，运行 `cargo run -p medkit-data -- schema` 重新生成 Schema。

在 Windows 上还可以：

```powershell
# 直接运行脚本，看它输出了什么
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\checks\disk\system-free-space.ps1 | ConvertTo-Json -Depth 5

# 用真实的 Windows PowerShell 5.1 跑所有检测，并对每个修复做
# 「制造故障 → 检测 → 修复 → 复查 → 撤销 → 复查」的往返测试（需要管理员权限，会临时改动本机设置）
cargo test -p medkit-core --test windows -- --include-ignored
```

CI 会在 Windows 上跑同样的测试。CI 的 Windows 机器是服务器版虚拟机，个别检测在那里本来就查不了（比如只判断桌面版支持期限的检测），这些检测的 ID 列在 `.github/workflows/ci.yml` 的 `MEDKIT_SMOKE_EXPECTED_FAILURES` 里，并写明原因；它们报的脚本错误只提示、不算失败。超时、脚本宿主出错、返回了没定义的结果代码，不管在不在名单里都算失败。

## 5. 提交前检查清单

- [ ] 写了 `references`，而且链接能打开。
- [ ] 脚本只有 ASCII，能在 PowerShell 5.1 上运行。
- [ ] 检测脚本可能返回的每个结果代码，在 YAML 里都有定义。
- [ ] 修复能撤销；不能撤销的写了 `irreversible_reason`。
- [ ] 界面文字是写给普通人看的，没有术语堆砌，也没有吓唬人的说法。
- [ ] `cargo run -p medkit-data -- check` 通过。
- [ ] 提交带了 `Signed-off-by`（`git commit -s`）。
