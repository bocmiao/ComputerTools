# 架构与数据格式规范

> 这份文档是代码的「合同」：数据文件怎么写、脚本怎么和引擎通信、界面怎么调用后端，都以这里为准。
> 背景和取舍见[《执行计划书》](plan.md)第六节；写新功能的步骤见[《功能编写指南》](feature-authoring.md)。

## 1. 仓库结构

```
catalog/                 数据（GPL-3.0，随程序发布）
  checks/                只读检测：体检和症状诊断都用它
  features/              原子修复：一个文件一个功能
  symptoms/              症状诊断树
  profiles/              检测清单（例如体检）
  tools/                 小工具：看信息、一键处理、打开系统工具（第 11 节）
scripts/                 PowerShell 脚本（只能有 ASCII 字符）
  host/Host.ps1          常驻宿主进程
  checks/…               检测脚本
  features/…             功能脚本（检测 / 执行 / 撤销 / 故障制造）
  tools/…                小工具脚本
data/                    知识数据（CC BY-SA 4.0）：同义词、错误码解释等
crates/medkit-core/      Rust 引擎：模型、目录加载与校验、原语、快照撤销、修改日志、检测
crates/medkit-data/      命令行工具：校验 catalog、生成 JSON Schema、打包
src-tauri/               Tauri 外壳：只把引擎的少数命令暴露给界面
app/                     Vue 3 界面
schema/                  由 Rust 类型自动生成的 JSON Schema（给编辑器补全用，不要手改）
docs/                    文档
```

## 2. 通用约定

- **ID**：小写字母、数字，用 `.` 或 `-` 连接，例如 `disk.system-free-space`。全局唯一，**永不改名**（预设套餐和药方都靠它引用）。正则：`^[a-z0-9]+([.-][a-z0-9]+)*$`。
- **本地化文本**：写成语言到文本的映射，必须有 `zh-CN`。

  ```yaml
  title: { zh-CN: 显示文件扩展名 }
  ```
- **schema_version**：目前都是 `1`。
- 一个数据文件只描述一个对象。文件放在哪个目录决定它的类型，文件名随意，建议和 ID 对应。

## 3. 检测（`catalog/checks/**/*.yaml`）

检测是只读的：它只看，不改。脚本返回一个**结果代码**和一组**事实**，界面上显示的文字写在 YAML 里（脚本里不写中文）。

```yaml
id: disk.system-free-space
schema_version: 1
title: { zh-CN: C 盘剩余空间 }
description: { zh-CN: 看看系统盘还剩多少空间。 }
category: disk               # disk / network / system / security / hardware / boot / printer …
requires_admin: false
timeout_sec: 15              # 可省略，默认 15
user_hive: false             # true 时引擎会传 -UserHive 参数（见 5.2）
probe:
  script: checks/disk/system-free-space.ps1    # 相对于 scripts/
  # 或者 builtin: cpu-features                  # 由 Rust 实现的内置检测
results:
  ok:
    status: ok
    message: { zh-CN: "C 盘还剩 {free_gb} GB，空间充足。" }
  low:
    status: advice
    message: { zh-CN: "C 盘只剩 {free_gb} GB（{free_pct}%），可能影响更新和软件运行。" }
    fixer: medkit
    next: { zh-CN: 打开「C 盘满了」，看看哪些东西可以清理或搬走。 }
    links: [symptom:disk-full]
references:
  - https://learn.microsoft.com/…
```

- **status**（显示给用户的结论）：
  - `ok`：正常
  - `advice`：建议处理
  - `manual`：需要人工
  - `unknown`：没查出来
  - `na`：不适用。例如台式机没有电池，这种结果不显示。
- **fixer**（谁能修）：
  - `medkit`：小药箱能修
  - `system`：用系统自带功能
  - `user`：用户自己在设置里改
  - `helper`：找懂哥
  - `vendor`：品牌售后
  - `isp`：运营商
  - `hardware`：需要换硬件
- **message / next** 里的 `{名字}` 会被替换成脚本返回的同名事实。数字最多保留一位小数。
- **links**：可以写 `symptom:<id>`、`feature:<id>` 或 `tool:<id>`（第 11 节），界面会显示成跳转或打开的按钮。
- 脚本返回的结果代码必须在 `results` 里有定义；没定义的，引擎按 `unknown` 处理。CI 里的冒烟测试会检查这一点。

### 内置检测（builtin）

| 名字 | 事实 | 结果代码 |
|---|---|---|
| `cpu-features` | `popcnt`、`sse42`、`windows11`（布尔值） | `ok`（都支持）、`missing`（缺任意一项，而且装的是 Windows 11）、`missing-win10`（缺，但装的是 Windows 10 或服务器版，只做提示） |
| `clock` | `today`（电脑上的日期）、`build_date`（这个版本的构建日期），都是 `YYYY-MM-DD` | `ok`、`behind`（电脑上的日期比构建日期早一天以上，时间肯定错了） |

`clock` 的构建日期：编译时的环境变量 `SOURCE_DATE_EPOCH`（CI 构建安装包时设成提交时间），没有时用 `builtin.rs` 里写死的日期（发版前顺手改成最近的日期）。这个日期只能早不能晚，晚于真实日期会让所有人都被误报；单元测试会拦住写成将来日期的情况。

## 4. 功能（`catalog/features/**/*.yaml`）

功能是一个原子修复，分两类：

- **原语类**：只由引擎原生支持的几种操作组成，撤销自动完成。
- **脚本类**：执行、检测、撤销都由脚本完成。

### 4.1 原语类

```yaml
id: explorer.show-extensions
schema_version: 1
title: { zh-CN: 显示文件扩展名 }
description: { zh-CN: 在资源管理器里显示 .docx、.exe 这类扩展名，更容易认出伪装成文档的病毒。 }
category: settings
risk: safe                   # safe / caution / danger
level: light                 # light / medium / heavy
recommend: recommended       # recommended / optional / not-recommended
subjective: false            # true 表示个人偏好，而不是建议
requires_admin: false
reboot: explorer             # none / explorer / logoff / reboot
target: current-user         # current-user / machine
applies_to:                  # 可省略
  min_build: 10240
  max_build: null
  editions: []               # 空表示所有版本；可选值：home / pro / enterprise / education
actions:
  - registry:
      key: HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced
      name: HideFileExt
      type: dword
      value: 0
windows_default:             # Windows 的默认状态：用于「恢复默认」，测试时也用它来制造故障
  - registry:
      key: HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced
      name: HideFileExt
      type: dword
      value: 1
undo: auto
references:
  - https://learn.microsoft.com/…
```

- `break_actions`（可省略）：测试时用来制造「故障状态」的原语列表。不写时，测试直接用 `windows_default`。
  - 什么时候需要写：功能要修的状态不是系统默认状态的时候。例如「关掉失效的代理」，故障状态是「代理开着、指向一个没人监听的端口」，而系统默认状态是「没有代理」。
- YAML 小心：以 `{` 或 `[` 开头的字符串（例如 CLSID `{20D04FE0-…}`）**必须加引号**，否则会被解析成映射或数组。

**registry 原语**

- `key`：必须以 `HKCU\` 或 `HKLM\` 开头。
  - `target: current-user` 时只能用 `HKCU\`。引擎会把它解析成**登录用户**的 `HKU\<SID>\…`，而不是提权后的账户。
  - `target: machine` 时只能用 `HKLM\`。
- `name`：值的名字；`""` 表示默认值。
- `type`：`dword` / `qword` / `string` / `expand-string` / `multi-string` / `binary`。
- `value`：
  - `dword` / `qword` 写数字；
  - `string` 类写字符串；
  - `multi-string` 写字符串数组；
  - `binary` 写十六进制字符串，例如 `"0200000000000000"`。
- `delete: true`：表示「这个值不应该存在」，这时不写 `type` 和 `value`。
- 写入时如果路径上的键不存在，引擎会创建，并在修改日志里记下创建了哪些键；撤销时，这些键如果已经空了，会一并删除。

**service 原语**

```yaml
- service:
    name: WSearch
    start_type: manual       # auto / delayed-auto / manual / disabled
```

只改启动类型，不启动也不停止服务。

**注册表黑名单**：CI 会拒绝写入以下位置。这些位置对应计划书第五节「不做」的事：

- `HKLM\SOFTWARE\Policies\Microsoft\Windows Defender`
- `HKLM\SOFTWARE\Microsoft\Windows Defender`
- `…\Image File Execution Options`
- `…\SafeBoot`
- `HKLM\SYSTEM\CurrentControlSet\Services\WinDefend`
- `…\Policies\Microsoft\Windows\WindowsUpdate` 下的 `NoAutoUpdate`
- UAC 的 `EnableLUA`
- SMB1 相关键

### 4.2 脚本类

```yaml
id: disk.hibernation-reduce
# …（其余字段同上，不写 actions）
detect: { script: features/disk/hibernation-detect.ps1 }   # 返回 { state, facts }
prepare: { script: features/disk/hibernation-reduce.ps1 }  # -Prepare $true，只返回 { before }
run:    { script: features/disk/hibernation-reduce.ps1 }   # 接收 -Before，返回 { after, facts }
undo:   { script: features/disk/hibernation-restore.ps1 }  # 接收 -Before '<JSON>'
break:  { script: features/disk/hibernation-break.ps1 }    # 只在测试中使用
```

- 检测脚本的 `state` 取值：`applied` / `not-applied` / `partial` / `unknown`。
- 可撤销脚本必须先用 `prepare` 读取原状态。引擎先把 `before` 写进修改日志，再把它以 `-Before` 传给执行脚本。执行脚本应在写入前核对快照仍然有效；撤销时，引擎把同一快照传给撤销脚本。
- 不可撤销的功能写 `undo: none`，同时必须写 `irreversible_reason`。

### 4.3 可选字段

- `verify: <check-id>`：修完以后，改用某个检测来复查。不写时，原语类按目标值比对，脚本类跑 `detect`。

## 5. 脚本约定

### 5.1 写法

- **只能有 ASCII 字符**，界面上的文字一律放在 YAML 里。这样不管系统是什么代码页、文件有没有 BOM，都不会乱码。
- 开头写 `[CmdletBinding()] param(...)`。
- 只向成功流输出**一个对象**（`[pscustomobject]` 或 hashtable）。原生命令的输出要接住，比如 `$null = powercfg ...` 或 `| Out-Null`。
- 日期一律输出成字符串，例如 `(Get-Date).ToString('o')`。PowerShell 5.1 的 `ConvertTo-Json` 会把日期转成 `\/Date(...)\/`。
- 读注册表、WMI、事件日志这类结构化数据，**不解析命令输出的文字**，因为中英文系统的措辞不一样。
- 必须能在 Windows PowerShell 5.1 上运行，不依赖 PowerShell 7。
- **禁止**（CI 会检查）：
  - `Write-Host`
  - `Invoke-Expression` / `iex`
  - `DownloadString` / `DownloadFile` / `Invoke-WebRequest` / `Invoke-RestMethod` / `Start-BitsTransfer` / `Net.WebClient`
  - `-EncodedCommand` / `FromBase64String`
  - `Set-MpPreference` / `Add-MpPreference`
  - `vssadmin` / `wevtutil cl` / `bcdedit`

### 5.2 参数

- `user_hive: true` 的检测，以及 `target: current-user` 的脚本类功能，会收到 `-UserHive`。它的值是登录用户的注册表根，例如 `Registry::HKEY_USERS\S-1-5-21-…`。脚本用 `Join-Path $UserHive 'Software\…'` 读写。
- 撤销脚本会收到 `-Before`：执行脚本当初返回的 `before`，格式是 JSON 字符串。

### 5.3 宿主协议（`scripts/host/Host.ps1`）

引擎只启动一个 `powershell.exe` 宿主进程，通过标准输入和标准输出一行一行地收发 JSON，编码为 UTF-8。

```
→ {"id":1,"script":"checks/disk/system-free-space.ps1","args":{"UserHive":"Registry::HKEY_USERS\\S-1-5-21-…"}}
← {"id":1,"ok":true,"data":{"result":"low","facts":{"free_gb":12.3,"free_pct":5.1}},"ms":84}
← {"id":2,"ok":false,"error":"Access is denied.","ms":12}
```

- `script` 必须是 `scripts/` 下的相对路径，不能含 `..`，宿主会再检查一次。
- 宿主会丢弃信息流、警告流、详细流和调试流，只收集成功流。所以脚本里误写的 `Write-Host` 不会破坏协议，但 CI 仍然会拒绝它。
- 超时由引擎控制：超时后结束宿主进程，下次调用时自动重启。
- 脚本不是从安装目录直接运行的。引擎把内嵌在 exe 里的脚本解压到只有管理员能写的运行目录，**每次执行前都校验 SHA-256**。

## 6. 症状（`catalog/symptoms/*.yaml`）

```yaml
id: network
schema_version: 1
title: { zh-CN: 上不了网 }
summary: { zh-CN: 网页打不开、微信能用但浏览器不行、Wi-Fi 连上了却没网。 }
keywords: [没网, 断网, wifi连不上, 网页打不开, 微信能用网页打不开]
causes:
  - { zh-CN: 代理设置残留（梯子、加速器卸载后没清干净） }
maturity: semi               # guide（图文指引）/ semi（半自动）/ one-click（一键修复）
steps:                       # 按顺序检查
  - check: network.proxy-dead
    stop_on: [advice, manual] # 这一步出问题就不再往下查（可省略）
    fixes: [network.proxy-off]
guide: { zh-CN: "…手动步骤…" }   # 可省略；maturity 是 guide 时必填
```

## 7. 检测清单（`catalog/profiles/*.yaml`）

```yaml
id: healthcheck
schema_version: 1
title: { zh-CN: 系统体检 }
checks: [disk.system-free-space, system.pending-reboot]
```

## 8. 修改日志与撤销

- 位置：`%ProgramData%\Medkit\journal\journal.jsonl`，一行一条 JSON，只追加不修改，每条写完立即落盘。
  - 目录的访问控制设为只有 SYSTEM 和 Administrators 能写。
  - 启动时如果这个目录是链接（junction / 符号链接），或者所有者不是 SYSTEM / Administrators，就把它改名挪走，重新建一个。
  - 读的时候按字节切行、每行单独解码，跳过坏掉的行（比如断电造成的半行，或者截断在中文字符中间的行），不影响其他记录。
  - 追加前先看文件末尾是不是换行；不是（上次写到一半断电），就先补一个换行，免得新记录和半行粘在一起被一起丢掉。
  - 同一时间只允许开一个小药箱（全局命名互斥体），修改日志只有一个写入者。
- **先记后改**。每个原语写两条记录：
  - 改之前写 `apply`：打算改什么、改之前是什么样；
  - 改完写 `commit`：用 `ref` 指向 `apply`，带上结果和读回来的值。

  ```json
  {"kind":"apply","v":1,"id":"…","session":"…","time":"2026-09-26T10:00:00Z","feature":"explorer.show-extensions","action":0,
   "target":{"registry":{"root":"HKU\\S-1-5-21-…","key":"Software\\…\\Advanced","name":"HideFileExt"}},
   "before":{"registry":{"value":{"type":"dword","data":1},"created_keys":[]}}}
  {"kind":"commit","v":1,"ref":"<apply 的 id>","time":"…","ok":true,
   "after":{"registry":{"value":{"type":"dword","data":0},"created_keys":[]}},"error":null}
  ```

  - `target` 的三种形式：`registry {root,key,name}`、`service {name}`、`script {feature,hive}`。`root` 是 `HKLM`、`HKCU` 或 `HKU\<SID>`；`hive` 是执行时传给脚本的 `-UserHive`，撤销时原样传回，不按撤销那一刻的登录用户重新解析。
  - `before` / `after` 对应的三种形式：`registry {value,created_keys}`（`value` 为 `null` 表示值不存在）、`service {start_type}`、`script {data}`。
  - 可撤销脚本类功能的 `apply.before` 是执行前快照；旧版记录仍可能把它留空、把原状态写在 `commit.after.script.data.before`，读取时兼容旧记录。
  - 如果改完以后写不进 `commit`（比如磁盘满了），立即把这一项退回原样。宁可不改，也不留下没有记录的改动。
  - 如果程序在两步之间崩溃，日志里只有 `apply`。界面上显示为「状态不确定」，仍然可以按 `before` 恢复（跳过核对）。
- 撤销时写一条 `undo` 记录：

  ```json
  {"kind":"undo","v":1,"id":"…","session":"…","time":"…","ref":"<apply 的 id>","reason":"user","forced":false,"ok":true,"error":null}
  ```

  `reason` 是 `user`（用户点了恢复）或 `rollback`（同一功能里后面的步骤失败，自动退回）。
- **不信返回值，读回来核对**：退回和撤销做完以后，都把目标位置读出来和原值比，一致才算成功；不一致的如实报告，条目保持「可以恢复」。
- **改的是另一个账户、而那个账户没有登录时**，撤销直接拒绝并说明原因：那时写到 `HKU\<SID>` 什么都改不到。
- 可撤销脚本在执行中途退出、没写 `commit` 的，仍可用 `apply.before` 恢复；旧版没有快照的记录不能自动恢复。
- **一个功能里的多个原语是一个整体**：中途有一个失败，前面已经改过的会按倒序自动退回；失败的那一步本身也会按原值退回（它可能改了一半，比如键建好了、值没写进去）。
- **已经是目标状态的原语不动**，也不写日志。整个功能本来就是好的，就直接返回「不用改」，也不建还原点。
- **为了写值而新建的键**记在 `created_keys` 里；撤销时，这些键如果已经空了，就从深到浅删掉；里面后来有了别的内容的，保留。删键只是收尾，失败了不影响值已经恢复。
- **撤销前先核对**：当前值不等于 `after`（被用户或别的软件改过）时，默认不撤销，返回 `drift: true`，由界面询问用户后再用 `force` 重试。
- **按会话撤销**：同一会话里、还能撤销的记录按倒序逐条撤销。倒序是为了同一个位置被改过多次时能一路核对回去。发生漂移或出错的条目跳过，单独报告，不影响其他条。
- 不能撤销的脚本类功能（`undo: none`）的记录，`canUndo` 为 `false`，撤销请求直接拒绝。
- **还原点**：`risk` 是 `caution` 或 `danger` 的功能，执行前先尝试创建还原点（`scripts/host/restore-point.ps1`）。创建失败不阻止执行，但会写进执行结果。Windows 默认 24 小时内只允许建一个，这种情况也会如实告诉用户。

## 9. 界面与后端的接口（Tauri 命令）

前端只能调用下面这些命令。系统诊断与修复命令只接受 ID，不接受命令字符串或路径。批量重命名的目录由后端的系统文件夹选择器取得，前端只能传文件名前缀。

| 命令 | 参数 | 返回 |
|---|---|---|
| `system_info` | — | `SystemInfo` |
| `catalog_summary` | — | `CatalogSummary` |
| `symptom_detail` | `id` | `SymptomDetail` |
| `run_profile` | `id` | `CheckResult[]` |
| `run_check` | `id` | `CheckResult` |
| `feature_detect` | `id` | `FeatureState` |
| `feature_preview` | `id` | `Preview` |
| `feature_apply` | `id` | `ApplyResult` |
| `journal_list` | — | `JournalSession[]`（新的在前） |
| `journal_undo` | `entryId`、`force` | `UndoResult` |
| `journal_undo_session` | `sessionId` | `UndoResult[]` |
| `report_generate` | — | `string`（已脱敏的纯文本） |
| `tool_run` | `id` | `ToolResult`（只能用于 `info`、`action` 小工具） |
| `tool_open` | `id` | `null`（只能用于 `open` 小工具；打不开时返回错误字符串） |
| `startup_list` | — | `StartupItem[]`（登录用户和所有用户的 Run 项、「启动」文件夹，开关状态和任务管理器一致） |
| `startup_set` | `id`、`enabled` | `ApplyResult`（停用或恢复；只接受最近一次列表里的 ID，记进修改日志） |
| `rename_select_folder` | — | `string \| null`（系统对话框选定的目录；取消返回 null） |
| `rename_preview` | `prefix` | `RenamePreview`（最多 500 个直属普通文件的原名与目标名） |
| `rename_apply` | — | `number`（执行已预览且文件列表与属性未变化的重命名数量） |

开机启动项（`crates/medkit-core/src/startup.rs`）：

- **列出**：脚本 `scripts/startup/list.ps1`（只读）列出登录用户的 Run、HKLM 的 Run 和它的 32 位路径、登录用户和所有用户的「启动」文件夹，给出每一项实际启动的程序（快捷方式会解析，rundll32、wscript 这类宿主程序取它运行的文件）、文件说明、公司和数字签名。路径只在本机界面显示，不进诊断报告。
- **开关**：和任务管理器「启动应用」是同一个：`HKCU` 或 `HKLM` 的 `Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`、`Run32`、`StartupFolder` 下和启动项同名的二进制值。第一个字节是单数（03，后面 8 个字节是停用的时间）表示停用；没有这个值、或者第一个字节是双数（02）表示启用。引擎自己读写这个值，只写这 5 个位置（写在代码里），只停用、不删除启动项本身，所以卸载小药箱以后在任务管理器里也能改回来。
- **修改日志**：每次开关记一条（功能 ID 是 `startup`），撤销时核对这个值有没有被别人（比如任务管理器）改过。以前的版本用删除 Run 值来停用（功能 ID `boot.startup-disable`），那些记录照样能撤销。
- **建议**：杀毒软件、输入法、Windows 自带的组件、硬件驱动和电脑厂商的功能标「建议保留」，其他的「不需要一开机就用的话，可以停用」；不说「建议停用」（原则 7）。
- 应用商店的应用有自己的开关（`ms-settings:startupapps`），任务计划不在这里管。

TypeScript 类型如下（字段名是 camelCase，所有文本已经渲染成中文）：

```ts
export type Status = 'ok' | 'advice' | 'manual' | 'unknown' | 'na'
export type Fixer = 'medkit' | 'system' | 'user' | 'helper' | 'vendor' | 'isp' | 'hardware'
export type Risk = 'safe' | 'caution' | 'danger'
export type Level = 'light' | 'medium' | 'heavy'
export type Recommend = 'recommended' | 'optional' | 'not-recommended'
export type Reboot = 'none' | 'explorer' | 'logoff' | 'reboot'
export type Maturity = 'guide' | 'semi' | 'one-click'
export type FeatureStateKind = 'applied' | 'not-applied' | 'partial' | 'unknown'

export interface SystemInfo {
  osCaption: string; build: number; edition: string
  isAdmin: boolean; interactiveUser: string | null; elevatedUserMismatch: boolean
  appVersion: string; catalogVersion: string
}
export interface ProfileSummary { id: string; title: string; checkCount: number }
export interface SymptomSummary { id: string; title: string; summary: string | null; keywords: string[]; maturity: Maturity }
export interface FeatureSummary {
  id: string; title: string; description: string; category: string
  risk: Risk; level: Level; recommend: Recommend; subjective: boolean; reboot: Reboot
  reversible: boolean; irreversibleReason: string | null
  applicable: boolean                  // 这台电脑能不能用（系统版本、Windows 版本不对就不能）
  notApplicableReason: string | null   // 不能用的原因，给用户看
}
export interface CatalogSummary {
  profiles: ProfileSummary[]; symptoms: SymptomSummary[]; features: FeatureSummary[]
  tools: ToolSummary[]
}
export interface SymptomStep { check: string; checkTitle: string; stopOn: Status[]; fixes: FeatureSummary[] }
export interface SymptomDetail extends SymptomSummary { causes: string[]; guide: string | null; steps: SymptomStep[] }
export interface CheckResult {
  id: string; title: string; category: string
  status: Status; resultCode: string | null; message: string
  fixer: Fixer | null; next: string | null; links: string[]
  facts: Record<string, unknown>; error: string | null; durationMs: number
}
export interface FeatureState { id: string; state: FeatureStateKind; details: string[]; error: string | null }
export interface PreviewChange { target: string; current: string; planned: string }
export interface Preview { feature: FeatureSummary; changes: PreviewChange[]; willCreateRestorePoint: boolean; notes: string[] }
export interface ApplyResult {
  feature: string; sessionId: string; entryIds: string[]; ok: boolean
  verified: FeatureStateKind; message: string; reboot: Reboot; notes: string[]; error: string | null
}
// ApplyResult 的三种结果，界面要分开显示：
//   ok && verified === 'applied'   已经改好、复查也确认生效
//   ok && verified !== 'applied'   改了，但复查没确认生效（message 会说明）
//   !ok                            没改成，已经退回原样；这时 reboot 一定是 'none'
// ok && entryIds 为空：本来就是好的，什么都没改
export interface JournalEntryView {
  id: string; sessionId: string; time: string; feature: string; featureTitle: string
  target: string; before: string; after: string; ok: boolean
  pending: boolean              // 程序在改的过程中退出，状态不确定
  undone: boolean; undoneAt: string | null
  canUndo: boolean              // 界面据此决定显不显示「恢复原状」
  error: string | null
}
export interface JournalSession { id: string; startedAt: string; entries: JournalEntryView[] }
export interface UndoResult {
  entryId: string; ok: boolean; drift: boolean; message: string; error: string | null
  reboot: Reboot               // 恢复以后要做什么才看得到效果；没恢复成功时是 'none'
}

// 小工具（第 11 节）
export type ToolGroup = 'info' | 'action' | 'open'
export type ToolOpens = 'program' | 'settings'
export type Audience = 'everyone' | 'helper'
export interface ToolSummary {
  id: string; title: string; description: string; category: string
  group: ToolGroup
  opens: ToolOpens | null      // 只有 open 有值：打开的是系统工具，还是「设置」里的一页
  audience: Audience           // helper：给懂哥用的，界面上标出来
  confirm: string | null       // 只有 action 可能有：执行前要用户确认的说明
}
export interface ToolRow { label: string; value: string; secret: boolean }   // secret：默认遮住，不进「复制全部」
export interface ToolSection { title: string; rows: ToolRow[] }
export interface ToolResult {
  id: string; title: string
  status: Status; resultCode: string | null; message: string; next: string | null; links: string[]
  sections: ToolSection[]      // 只有 info 有内容
  error: string | null; durationMs: number
}
```

## 10. 安全边界（摘要）

- 界面层把所有来自系统的字符串都当作不可信内容：不用 `v-html`，开启严格的 CSP，不加载任何远程资源。
- 后端不开本地服务，不注册 URL 协议，不装内核驱动。
- 脚本和数据都内嵌在 exe 里；运行目录只有管理员能写，执行前逐个校验哈希。
- 其余要求见计划书 6.5 节。

## 11. 小工具（`catalog/tools/**/*.yaml`）

小工具是一次性的操作，**不改设置**，所以不写修改日志，也没有撤销。分三组：

| group | 做什么 | 怎么执行 | 例子 |
|---|---|---|---|
| `info` | 看信息（只读） | 跑脚本，结果显示成几张「标签：值」的小表 | 电脑配置、WiFi 密码 |
| `action` | 一键处理 | 跑脚本，结果显示成一句话（和检测一样用结果代码） | 刷新 DNS 缓存、重启资源管理器 |
| `open` | 打开系统自带的工具，或「设置」里的某一页 | 引擎直接打开，不跑脚本 | 任务管理器、Windows 更新 |

**会改设置的（哪怕能撤销）一律做成功能（第 4 节），不能做成小工具**：小工具不进修改日志，改了就没法撤销。
`action` 只能做没有持久影响的事，比如清缓存、重启一个进程。

### 11.1 公共字段

```yaml
id: network.flush-dns        # 规则和检测、功能的 ID 一样；检测结果里用 tool:<id> 链接过来
schema_version: 1
group: action                # info / action / open
title: { zh-CN: 刷新 DNS 缓存 }
description: { zh-CN: …… }   # 一两句话：做什么、什么时候用
category: network            # system / network / disk / hardware / settings ……
audience: everyone           # everyone（默认）/ helper：给懂哥用的
references: [ … ]
```

### 11.2 `info` 和 `action`

```yaml
requires_admin: true
timeout_sec: 30              # 1 到 600，默认 15
user_hive: false             # 为 true 时给脚本传 -UserHive（和检测一样）
run: { script: tools/network/flush-dns.ps1 }
confirm: { zh-CN: …… }       # 只有 action 能写：执行前确认框里的话；不写就不确认
results:                     # 和检测的 results 完全一样：结果代码 → status、message、next、links
  done:
    status: ok
    message: { zh-CN: "已经刷新了 DNS 缓存（清掉了 {entries} 条记录）。" }
labels:                      # 只有 info 能写：表格里的文字（脚本里不能写中文）
  sections: { cpu: { zh-CN: 处理器 } }
  rows: { name: { zh-CN: 型号 } }
  values: { ssd: { zh-CN: 固态硬盘 } }
```

脚本和检测脚本一样只输出一个对象，另外 `info` 脚本可以带 `sections`：

```json
{
  "result": "ok",
  "facts": { "count": 2 },
  "sections": [
    { "id": "cpu", "rows": [ { "id": "name", "value": "Intel(R) Core(TM) i5-8250U CPU @ 1.60GHz" } ] },
    { "id": "disk", "name": "Samsung SSD 870 EVO 500GB",
      "rows": [ { "id": "size", "value": "466 GB" }, { "id": "type", "code": "ssd" } ] },
    { "id": "wifi", "name": "HomeWiFi",
      "rows": [ { "id": "password", "value": "12345678", "secret": true } ] }
  ]
}
```

- 表格标题是 `labels.sections[id]`，有 `name` 时接上「：name」，比如「硬盘：Samsung SSD 870 EVO 500GB」。
- 每一行的标签是 `labels.rows[id]`，同一个 id 可以出现多行。值要么是 `value`（字符串或数字，原样显示），要么是 `code`（显示 `labels.values[code]`）。
- `secret: true` 的值默认遮住，点「显示」才看得到，也不会被「复制全部」带上。
- 用到了 `labels` 里没有的 id 或 code，界面照样显示原文，同时在 `error` 里说明；CI 的冒烟测试会拦住这种情况。
- **不输出序列号、MAC 地址、电脑名、用户名**：界面有「复制全部」，用户会把它发给别人。

### 11.3 `open`

二选一：

```yaml
open: { program: device-manager }   # 系统工具：名字必须在下面的名单里
open: { settings: windowsupdate }   # 「设置」里的一页（ms-settings:<页面>）：页面也必须在名单里
```

名单写在 `crates/medkit-core/src/tools.rs` 里，改名单要改代码、过代码审核，数据文件里不能随便写程序路径：

| program | 实际启动（都在 System32 下，按绝对路径） |
|---|---|
| `task-manager` | `Taskmgr.exe` |
| `device-manager` | `mmc.exe devmgmt.msc` |
| `disk-management` | `mmc.exe diskmgmt.msc` |
| `disk-cleanup` | `cleanmgr.exe` |
| `system-restore` | `rstrui.exe` |
| `reliability` | `perfmon.exe /rel` |
| `memory-diagnostic` | `MdSched.exe` |
| `system-information` | `msinfo32.exe` |
| `services` | `mmc.exe services.msc` |
| `event-viewer` | `mmc.exe eventvwr.msc` |
| `control-panel` | `control.exe` |

settings 页面：`windowsupdate`、`storagesense`、`appsfeatures`、`defaultapps`、`network-status`、`printers`、
`sound`、`powersleep`、`display`、`bluetooth`、`recovery`、`windowsdefender`、`privacy-microphone`、`privacy-webcam`、`dateandtime`。

- 系统工具以小药箱的权限（管理员）启动，所以不会再弹一次 UAC；「设置」页面由系统打开。
- 精简系统上被删掉的工具，打开时如实说「这台电脑上没有这个工具」，不去别处找。

