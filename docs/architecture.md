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
- **fact_labels**（可省略）：给代码类的事实配中文说明。脚本只返回代码（例如错误代码 `0x80070005`、设备管理器的代码 `[10, 43]`），每个代码的说明写在 YAML 里，引擎查出来作为一个新的事实给模板用：

  ```yaml
  fact_labels:
    error_notes:               # 新事实的名字，模板里写 {error_notes}
      from: error_codes        # 脚本返回的事实：字符串、数字，或者它们的数组
      values:
        "10": { zh-CN: 代码 10 是设备启动不了，多半是驱动的问题。 }
        "43": { zh-CN: 代码 43 是设备自己报告出了问题、被 Windows 停用了。 }
  ```

  数组逐个查，有说明的按原顺序连起来；查不到的跳过，一个都查不到（或者脚本没返回这个事实）时模板里换成空字符串，结果的事实里也不加这一项。所以说明要写成带句号的完整句子，放在模板里一句话的开头或结尾。新事实的名字不能和 `from` 相同。
- **links**：可以写 `symptom:<id>`、`feature:<id>` 或 `tool:<id>`（第 11 节），界面会显示成跳转或打开的按钮；
  还可以写 `test:<名字>`，跳到工具箱「屏幕、键盘、鼠标、声音测试」里的那一项（修完试一试）。名字只能是
  `screen`、`mouse`、`keyboard`、`speaker`、`microphone`、`camera`（`crates/medkit-core/src/tools.rs` 的 `DEVICE_TESTS`，
  界面里对应 `labels.ts` 的 `DEVICE_TEST_LABELS` 和 `DeviceTests.vue` 里的 `device-test-<名字>`，测试会核对三处一致）。
- 脚本返回的结果代码必须在 `results` 里有定义；没定义的，引擎按 `unknown` 处理。CI 里的冒烟测试会检查这一点。

### 内置检测（builtin）

| 名字 | 事实 | 结果代码 |
|---|---|---|
| `cpu-features` | `popcnt`、`sse42`、`windows11`（布尔值） | `ok`（都支持）、`missing`（缺任意一项，而且装的是 Windows 11）、`missing-win10`（缺，但装的是 Windows 10 或服务器版，只做提示） |
| `clock` | `today`（电脑上的日期）、`build_date`（这个版本的构建日期），都是 `YYYY-MM-DD` | `ok`、`behind`（电脑上的日期比构建日期早一天以上，时间肯定错了） |
| `keyboard-aids` | `filter_keys`、`sticky_keys`、`mouse_keys`（布尔值：这次登录里实际开没开，用 SystemParametersInfo 读，不读注册表） | `ok`、`filter-keys`、`sticky-keys`、`mouse-keys`（几项都开着时按这个顺序报一项） |
| `winsock` | `entries`（一共几项）、`missing` / `missing_count`、`lsp` / `lsp_count`、`others` / `others_count`（名字是「产品名（文件名）」，用「、」隔开，没有路径） | `missing-file`（有组件的 DLL 不在了）、`no-tcpip`（64 位或者 32 位的目录里没有 IPv4 的 TCP 基础提供程序）、`lsp`（有第三方的分层服务提供程序）、`ok-others`（没有 LSP，有第三方的基础协议）、`ok`；先满足哪个算哪个 |
| `display-resolution` | `count`（正在用的显示器个数）、`summary`（每个显示器现在的分辨率，用「、」隔开）、`displays`（每个显示器一行：名字、现在的分辨率、推荐的分辨率、是不是在「复制」）；有显示器低于推荐的分辨率时，还有第一个这样的显示器的 `name`、`current`、`recommended` | `remote`（远程桌面里）、`no-display`、`not-recommended`（有显示器比推荐的分辨率低：宽或者高小一些，宽高比不一样的也算）、`cloned`（低于推荐的是在「复制」模式下）、`ok`（有显示器读得到推荐的分辨率，都不低于它）、`unknown`（都读不到）；先满足哪个算哪个 |

`winsock` 用微软公开的服务提供程序接口读 Winsock 目录（`crates/medkit-core/src/platform/winsock.rs`）：`WSCEnumProtocols` 列出每一项（包括隐藏的项和分层协议本身），`WSCGetProviderPath` 读 DLL 的路径；64 位 Windows 上 32 位程序用的那一份用 `WSCEnumProtocols32`、`WSCGetProviderPath32` 读（很多国产软件是 32 位的），路径里的 `%ProgramFiles%` 按 `Program Files (x86)`、System32 按 SysWOW64 解释。协议链长度不是 1 的是 LSP（微软 WSAPROTOCOLCHAIN 文档：0 是分层协议本身，大于 1 是经过它的协议链）；DLL 在 Windows 文件夹里、版本信息里的公司是 Microsoft 的算 Windows 自己的（放在 System32 里冒充系统文件的老式 LSP 照样算第三方）。结论在 `crates/medkit-core/src/winsock.rs`，路径只用来看文件在不在、读版本信息。

`display-resolution` 用微软的「连接和配置显示器」接口（`crates/medkit-core/src/platform/displays.rs`，做法照 MartinGC94/DisplayConfig，MIT）：`QueryDisplayConfig`（`QDC_ONLY_ACTIVE_PATHS`）读每个显示器现在的桌面大小（源模式）和显示方向，竖着放的把宽高换回来；`DisplayConfigGetDeviceInfo` 读推荐的分辨率（`DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE`，「设置」里标着「推荐」的就是它）和型号名、接口类型（`GET_TARGET_NAME`：EDID 里写的名字，不含序列号；接在内部接口上的是笔记本、一体机自带的屏幕）；几条路径用同一个源就是「复制」。比推荐的高（显卡的「超级分辨率」）不算问题。只检测、不改分辨率：改分辨率由用户在「设置」里点，Windows 会问要不要保留，不点自动改回去。结论在 `builtin.rs`。

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

- `key`：必须以 `HKCU\`、`HKLM\` 或 `HKU\.DEFAULT\` 开头。
  - `target: current-user` 时只能用 `HKCU\`。引擎会把它解析成**登录用户**的 `HKU\<SID>\…`，而不是提权后的账户。
  - `target: machine` 时只能用 `HKLM\` 或 `HKU\.DEFAULT\`。后者是还没有人登录时（登录界面）和系统账户用的那一份用户设置，不属于哪个用户，所以和 HKLM 一样算整台电脑的、要管理员（例如登录界面上 Num Lock 开不开：`HKU\.DEFAULT\Control Panel\Keyboard` 的 `InitialKeyboardIndicators`）。别的用户的 `HKU\<SID>` 一律不能写。
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
id: disk.reduce-hiberfile
# …（其余字段同上，不写 actions）
detect:  { script: features/disk/reduce-hiberfile-detect.ps1 }  # 返回 { state }（撤销前核对修改还在不在）
prepare: { script: features/disk/reduce-hiberfile-run.ps1 }     # -Prepare $true，只返回 { before }
run:     { script: features/disk/reduce-hiberfile-run.ps1 }     # 接收 -Before，返回 { after }；状态变了返回 { skipped: true }
undo:    { script: features/disk/reduce-hiberfile-undo.ps1 }    # 接收 -Before '<JSON>'
break:   { script: features/disk/reduce-hiberfile-break.ps1 }   # 只在测试中使用
verify:  disk.hiberfile                                         # 能不能执行、改好没有，看这个检测
```

`break` 脚本也会打进安装包。制造故障本身是第五节不许做的事时（比如禁用 Windows 更新服务），或者没法安全地制造故障时（重置 Winsock：得往测试机的 Winsock 里装一个 LSP），不写 `break`，把功能的 ID 加进 `catalog.rs` 的 `BREAK_IN_TESTS`：校验不再提示缺 `break`，通用的往返测试跳过它，由 `tests/windows.rs` 里的专门测试来测。

- 检测脚本的 `state` 取值：`applied` / `not-applied` / `partial` / `unknown`。
- 可撤销脚本必须先用 `prepare` 读取原状态。引擎先把 `before` 写进修改日志，再把它以 `-Before` 传给执行脚本。执行脚本应在写入前核对快照仍然有效；撤销时，引擎把同一快照传给撤销脚本。
- 不可撤销的功能写 `undo: none`，同时必须写 `irreversible_reason`。`detect` 只用在两处：没有 `verify` 时判断状态、撤销前核对修改还在不在，所以写了 `verify`、又撤销不了的可以不写 `detect`（比如 `network.winsock-reset`：状态看检测 `network.winsock`）。

### 4.3 可选字段

- `verify: <check-id>`：用某个检测判断这一项的状态（执行前、修完复查都用它）。不写时，原语类按目标值比对，脚本类跑 `detect`。
  检测的结论 ok 表示已经改好，advice、manual 表示还没改，unknown 表示没查出来；na 表示这台电脑用不了这一项
  （例如没有休眠文件、是笔记本）：预览里写明原因、不给执行，执行也会被拒绝，原因就是检测的结论。
  版本不对（`applies_to`）不用跑脚本就知道；「这台电脑有没有要改的东西、该不该改」要检测了才知道，就用这个办法。
  检测本身出错、超时（unknown）时引擎照常让执行（`network.proxy-off` 这类修复要靠它），所以执行脚本要按同样的规则
  再核对一遍，规则写在检测和执行脚本共用的代码块里（例如 `disk.reduce-hiberfile` 的 `Get-HiberBlock`）。

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
  - 下载：`DownloadString` / `DownloadFile` / `DownloadData` / `Invoke-WebRequest` / `Invoke-RestMethod` / `Start-BitsTransfer` / `Net.WebClient` /
    `[Net.WebRequest]::Create` / `HttpWebRequest` / `HttpClient` / `curl` / `wget` / `bitsadmin` / `certutil -urlcache`
  - `-EncodedCommand` / `FromBase64String`
  - `Set-MpPreference` / `Add-MpPreference`
  - `vssadmin` / `wevtutil cl` / `bcdedit`
- **唯一能下载的：从微软官网下载官方安装包**（现在只有小工具「安装微软 VC++ 运行库」）。计划书第五节第 8 条不做的是下载脚本来执行，
  这里下载的是微软签名的安装程序（计划书 4.2「软件打不开，提示缺少 xxx.dll」：用微软官网的官方运行库，绝不单独下载 DLL）。
  能下载的脚本写在代码里（`crates/medkit-core/src/lint.rs` 的 `DOWNLOAD_SCRIPTS`），改名单要过代码审核。这些脚本另有要求（前两条 CI 会检查）：
  - 网址只能以 `https://aka.ms/`、`https://download.microsoft.com/`、`https://download.visualstudio.microsoft.com/` 开头；跳转以后也要是 https 的 `*.microsoft.com`；
  - 用 `Get-AuthenticodeSignature` 核对下载的文件：签名有效、签名者是 Microsoft Corporation，才运行；
  - 下载到 `%SystemRoot%\Temp` 下新建的、只有 SYSTEM 和 Administrators 能打开的文件夹（没有管理员权限的程序换不了核对过签名的文件），用完删掉；
  - 先下载完、再动手：下载限时，失败了电脑上什么都不改；安装程序也要在小工具的超时以前结束或者不再等它（超时只会结束 PowerShell，
    结束不了它启动的安装程序）。

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
- 宿主启动时把模块路径（`PSModulePath`）设成只有 PowerShell 自己的模块目录（`$PSHOME\Modules`），脚本只用系统自带的命令。从父进程继承来的路径可能把别的目录排在前面：PowerShell 7 的模块在 5.1 里加载不了（签名检查会失败）；普通程序能写的目录，以管理员身份运行的工具不该从那里加载代码。
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
links: ["tool:settings.display", "symptom:screen-colors", "test:screen"]  # 可省略：手动步骤下面的按钮，给指引里提到的
                             # 小工具、相关症状、修完试一试的设备测试（只能是 tool:、symptom: 或 test:，修复写在 steps 的 fixes 里；
                             # 不能指向自己、不能重复；要有 guide）
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

前端只能调用下面这些命令。系统诊断与修复命令只接受 ID，不接受命令字符串或路径。批量重命名、图片批量处理、找大文件的目录，「文件删不掉」要查的文件，图片合成的 PDF 存到哪里，「U 盘里的文件不见了」要查的 U 盘，由后端的系统选择器取得，前端只能传改名规则（查找替换、序号、前后缀、扩展名）或者文件名和图片内容，拿不到、也传不了路径。每个命令都要登记在 `src-tauri/build.rs` 的 `COMMANDS` 和 `capabilities/main.json` 里，否则界面调不动（接线测试会查）。

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
| `report_generate` | `note?`（用户自己写的「遇到了什么问题」，最多 1000 字） | `string`（已脱敏的纯文本；`note` 放在最前面，一起脱敏） |
| `tool_run` | `id` | `ToolResult`（只能用于 `info`、`action` 小工具） |
| `tool_open` | `id` | `null`（只能用于 `open` 小工具；打不开时返回错误字符串） |
| `startup_list` | — | `StartupItem[]`（登录用户和所有用户的 Run 项、「启动」文件夹，开关状态和任务管理器一致） |
| `startup_set` | `id`、`enabled` | `ApplyResult`（停用或恢复；只接受最近一次列表里的 ID，记进修改日志） |
| `context_menu_list` | — | `ContextMenuItem[]`（右键菜单里第三方软件加的命令、外壳扩展和 Windows 11 新菜单里应用的项目，显示不显示由引擎自己读） |
| `context_menu_set` | `id`、`visible` | `ApplyResult`（拿掉或恢复；只接受最近一次列表里的 ID，记进修改日志；外壳扩展和应用的项目 `reboot` 为 `explorer`） |
| `new_menu_list` | — | `NewMenuItem[]`（右键「新建」菜单里的项：扩展名下的 ShellNew 键，机器的和登录用户的；文件夹、快捷方式、库不列；显示不显示由引擎按注册表里现在的值读） |
| `new_menu_set` | `id`（扩展名）、`visible` | `ApplyResult`（关掉：把 FileName、Command、Data、NullFile、Handler 这几个值改名成 `MedkitHidden.<原名>`，类型和数据不变；恢复：改回来。扩展名下所有 ShellNew 键一起改，每个值写新名字、删旧名字两条修改日志；只接受最近一次列表里的 ID；改完删掉资源管理器的「新建」菜单缓存，不记进日志） |
| `shell_places_list` | — | `ShellPlaceItem[]`（软件加在资源管理器导航栏最上面一层和「此电脑」里的图标：`Explorer\Desktop\NameSpace`、`Explorer\MyComputer\NameSpace` 下登记的 CLSID，机器的和登录用户的，同一个 CLSID 算一项；Windows 自己的基本位置不列；显示不显示由引擎按注册表里现在的值读） |
| `shell_places_set` | `id`（CLSID）、`visible` | `ApplyResult`（「此电脑」里有的：隐藏时在登录用户的 `Policies\NonEnum` 里写 `{CLSID}` = 1；只在导航栏里的：在登录用户的那份 CLSID 键里把 `System.IsPinnedToNameSpaceTree` 写成 0，32 位程序看的 WOW6432Node 那一份也一样。恢复见下面。只接受最近一次列表里的 ID，记进修改日志，`reboot` 为 `explorer`） |
| `key_remap_get` | — | `KeyRemapView`（能选的键：名字和浏览器 `KeyboardEvent.code` 一样，多媒体键只能当「变成」的键；现在 `Scancode Map` 里的改键；有小药箱认不出来的键或者格式时 `foreign` 为 true，认不出来的那些写进 `foreignText`） |
| `key_remap_set` | `mappings`（`{ from, to }`，`to` 为 null 是这个键不起作用） | `ApplyResult`（整张表换成这些，空的就删掉 `Scancode Map`；键只能是名单里的，同一个键不能改两次、不能改成自己，最多 24 个；现在的设置里有认不出来的键时只接受空的（全部恢复）；和现在一样时什么都不改；记进修改日志，`reboot` 为 `reboot`） |
| `rename_select_folder` | — | `string \| null`（系统对话框选定的目录；取消返回 null） |
| `rename_preview` | `rules` | `RenamePreview`（最多 500 个直属普通文件的原名、新名、名字变不变；不处理的文件数） |
| `rename_apply` | — | `number`（执行已预览、文件夹没有变化的改名，返回改了几个） |
| `rename_undo` | — | `number`（把上一次改名改回原名；文件改动过、原名被占用时不撤销） |
| `image_select_folder` | — | `string \| null`（系统对话框选定的保存目录；取消返回 null） |
| `image_save` | 请求体是图片内容（二进制，不是 JSON）；请求头 `x-medkit-name`（URL 编码的文件名）、`x-medkit-modified`（原图修改时间，毫秒） | `string`（实际用的文件名。只新建、不覆盖，重名加「 (2)」；只收 JPG、PNG、WebP，内容开头要和扩展名对得上） |
| `image_open_folder` | — | `null`（按「文件夹」类型交给资源管理器打开选定的保存目录） |
| `ocr_recognize` | 请求体是界面转好的 PNG（二进制，最大 64 MB） | `OcrView`（`status`：ok / no-language / unsupported / bad-image；认出来的文字一行一行，中文的字之间没有空格；用的识别语言、装了哪些、有没有中文、长图是不是只认了一部分。图片存成 `%ProgramData%\Medkit\ocr` 里的临时文件交给脚本，认完就删，同一时间只认一张） |
| `pdf_save` | 请求体是界面拼好的 PDF（二进制）；请求头 `x-medkit-name`（URL 编码的建议文件名） | `string \| null`（弹出系统的「另存为」对话框，存到用户选的地方，返回完整路径；点了取消返回 null。只收开头是 `%PDF-`、最后有 `%%EOF` 的内容；先写临时文件再换名，写失败时原来的同名文件不受影响；选的名字不是 .pdf 结尾的补上 .pdf，补出来的名字已经有文件时不存） |
| `pdf_reveal` | — | `null`（在资源管理器里打开刚存好的 PDF 所在的文件夹并选中它） |
| `long_image_save` | 请求体是界面拼好的长图（JPG 或 PNG，二进制）；请求头 `x-medkit-name`（URL 编码的建议文件名） | `string \| null`（弹出系统的「另存为」对话框，只列这一种图片，建议的名字换成它的扩展名；存到用户选的地方，返回完整路径；点了取消返回 null。只收完整的 JPG（FF D8 FF 开头、FF D9 结尾）和 PNG（签名开头、IEND 结尾），最大 300 MB；存法和 `pdf_save` 一样：先写临时文件再换名，选的名字没有扩展名时补上，补出来的名字已经有文件时不存） |
| `long_image_reveal` | — | `null`（在资源管理器里打开刚存好的长图所在的文件夹并选中它） |
| `hidden_pick_folder` | — | `HiddenReport \| null`（用系统的文件夹选择框选 U 盘；Windows 所在的盘报错。列出所选文件夹里直接的、带隐藏或系统属性的文件和文件夹，被藏起来的程序和脚本文件、病毒放的快捷方式单独列出；取消返回 null） |
| `hidden_rescan` | — | `HiddenReport \| null`（把上次选的再查一遍；没选过返回 null） |
| `hidden_restore` | `ids`（结果里的编号） | `HiddenRestore`（去掉这些文件、文件夹连同里面一切的隐藏、系统属性，原来带着的也去掉只读；程序和脚本文件照样藏着；原属性都记下来，返回改了多少和重新查的结果） |
| `hidden_undo` | — | `HiddenUndo`（把上一次「显示出来」改过的属性都改回去） |
| `disk_speed_drives` | — | `DriveView[]`（本机固定的和可移动的盘：盘符、卷标、文件系统、大小、剩余空间，剩余不到 2 GB 的 `canTest` 为 false） |
| `disk_speed_run` | `letter` | `SpeedResult`（在这个盘的根目录写一个关掉就删的临时文件，不经过系统缓存，测顺序写、顺序读、4 KB 随机读，最多写 1 GB、每步限时；只收现在列出来、能测的盘符，同一时间只测一个） |
| `exe_check_pick` | — | `ExeCheckView \| null`（「此应用无法在你的电脑上运行」：系统的选择框选一个程序文件，只读开头最多 1 MB，看它本身能不能在这台电脑上运行：`verdict` 是 empty / not-exe / truncated / dll / old16 / wrong-machine / not-desktop / ok，`guess`（不是程序时像什么：msi、压缩包、网页、PDF）、`machine`（给哪种处理器的）、`pc`（这台电脑的处理器，IsWow64Process2）、`windows11`、`console`、`dotnet`；只有文件名和大小，没有文件夹；取消返回 null） |
| `recycle_drives` | — | `RecycleDriveView[]`（本机固定的和可移动的盘上有没有回收站文件夹（`<盘>:\$Recycle.Bin`）、里面有多少个文件、一共多大：只有个数和大小，没有文件名，也没有按账户分的文件夹名（SID）；所有盘一共最多数 8 秒、一个盘最多数 20 万个文件，没数完的 `complete` 为 false；不跟着链接走，链接也不算） |
| `recycle_repair` | `letter` | `RecycleRepairView`（回收站坏了：删掉这个盘的 `$Recycle.Bin`，重启以后 Windows 重新建一个；里面所有账户的东西都删掉、找不回来，界面先确认。只收现在还在的盘的盘符；`outcome`：absent（本来就没有）/ done / partly（有的删不掉，`left` 是还剩几个文件）） |
| `screen_fullscreen` | `on` | `null`（屏幕坏点测试：窗口进入、退出全屏） |
| `awake_get` | — | `AwakeStatus`（`{ on, display }`：「别让电脑自己睡着」开没开） |
| `awake_set` | `on`、`display` | `AwakeStatus`（SetThreadExecutionState，只在小药箱开着时有效，不改电源设置） |
| `brightness_list` | — | `MonitorBrightness[]`（显示器亮度：每个显示器的 `id`（`<GDI 设备名>#<序号>`，插拔、换接口以后就变）、型号名（一块桌面上只接着一个显示器时才对得上）、是不是电脑自带的屏幕、现在的亮度 `percent`（0–100）。外接显示器用 DDC/CI 读，读不到的（笔记本自带的屏幕、虚拟机、显示器菜单里关了 DDC/CI）也列出来，`percent` 是 null） |
| `brightness_set` | `id`、`percent`（0–100） | `number`（调完以后显示器读回来的亮度。做法照 emoacht/Monitorian（MIT）：先用高级接口 SetMonitorBrightness，不支持时用 VCP 代码 0x10（SetVCPFeature），按显示器报的最小、最大值换算；有的显示器报成功其实没设上，所以设完读回来。找不到这个显示器、显示器不让调、调不上各有一句话。改的是显示器自己的亮度，和按显示器上的按钮一样，不记修改日志） |
| `shutdown_get` | — | `ShutdownStatus`（`{ plan: { at, restart } \| null }`：小药箱安排的定时关机；系统查不到别处安排的） |
| `shutdown_schedule` | `seconds`（60 到 24 小时加 60 秒）、`restart` | `ShutdownStatus`（InitiateSystemShutdownExW，到时间强制关掉程序，和 `shutdown /s /t` 一样；小药箱安排过的先取消再换成新的时间；已经有别处安排的就报错，不去动它） |
| `shutdown_cancel` | — | `ShutdownCancel`（`{ cancelled }`：AbortSystemShutdownW，不管是谁安排的；false 表示本来就没有安排） |
| `lockers_pick_files` | — | `FileLockReport \| null`（系统的选择框选文件，可以多选；查哪些程序在用它们，取消时 `null`） |
| `lockers_pick_folder` | — | `FileLockReport \| null`（选文件夹，查在用里面文件的程序） |
| `lockers_refresh` | — | `FileLockReport \| null`（再查一次上次选的；还没选过时 `null`） |
| `space_pick_folder` | — | `SpaceReport \| null`（系统的选择框选文件夹，找最大的文件和内容完全一样的文件；只读） |
| `space_rescan` | — | `SpaceReport \| null`（把上次选的文件夹再数一遍） |
| `space_reveal` | `id` | `null`（在资源管理器里打开所在文件夹并选中结果里的这个文件；只接受最近一次结果里的编号） |
| `popup_find` | `seconds`（最多 10） | `WindowOwnerReport`（等这么多秒，让用户把鼠标移到弹窗上，看鼠标指着的窗口是哪个程序的；只读） |
| `popup_reveal` | — | `null`（在资源管理器里打开 `popup_find` 最近一次找到的程序所在的文件夹并选中它；界面传不了路径） |

图片转文字（`app/src/components/OcrTool.vue`、`crates/medkit-core/src/ocr.rs`、`scripts/ocr/recognize.ps1`）：用 Windows 自带的文字识别 Windows.Media.Ocr（Windows 10 起都有），只能在 Windows PowerShell 5.1 里用（微软 PowerToys「文本提取器」的文档也这么说），正好是脚本宿主。图片在界面里转正、画到白底的画布上存成 PNG（透明的截图也认得出），后端存成只有管理员能写的临时文件交给脚本，认完就删；不联网、不上传。脚本优先用简体中文的识别，没有就用别的中文、用户的语言、装了的第一种；更宽的按比例缩到 `MaxImageDimension`（Windows Server 2025 上是 10000）；4000 像素以内的整张认，更高的（长截图）分成几块、每块最多 4000 像素、块和块之间重叠 400 像素来认：相邻两块的分界线在重叠的正中间，一块要它那部分里的行（按一行的中间算），再往重叠里多要 100 像素，被块的边切开的行留给看得到整行的那一块；前一块已经要了的同一行（中间离得不到较高那行的一半、至少 8 像素，左右有重叠）后一块不再要。同一行在两块里认出来的位置会差几个像素（Windows 上的测试差了 11 像素），只按分界线一刀切时，压在线上的行会认两次或者丢掉。Windows 上的测试在交界附近放字核对不丢、不重复。一块 10000 像素高时，测试里认出来的字丢了开头的字母、还整个丢了一个词，所以不用那么大的块。最多认 30 块。Windows 把中文的每个字当成一个词，引擎接起来时只在两边都不是中文（汉字、假名、中文标点、全角字符）时加空格。没有中文识别时界面给小工具「安装中文文字识别」（`Dism.exe /Online /Add-Capability` 装 `Language.OCR~~~zh-CN~0.0.1.0`，缺同一语言的 `Language.Basic` 时先装它；下载不了的原因和「安装 .NET Framework 3.5」一样只检测、不改）。

看报错截图（`app/src/components/ErrorShotSearch.vue`、`app/src/utils/symptomMatch.ts`）：「按症状修」的搜索框下面，粘贴或者选一张报错窗口的截图，用上面的 `ocr_recognize` 认出字，再和症状的名字、关键词对：和搜索框同一种写法（全角转半角、转小写、去掉空格和标点），错误代码里被认成字母 O 的 0 先换回来；一个症状里被另一个对上的关键词包含的只算长的那个，对上的字加起来至少 4 个才算，按字数排，最多给 3 个。认出来的 0x 开头的八位错误代码也单独列出来。常见报错的原话（缺 DLL、0xc000007b、0x80070035、打印机 11b、更新 0x800f0922、ERR_CONNECTION_RESET、蓝屏终止代码、「任务管理器已被系统管理员停用」、代码 10、43……）要对上该对的症状、而且排第一：`crates/medkit-core/tests/repo_catalog.rs` 用真实数据核对（和界面同样的算法）。

图片批量处理（`app/src/components/BatchImageTool.vue`、`src-tauri/src/images.rs`）：解码、缩放、裁剪、编码都在界面里用 WebView2 自带的解码器和画布做（不加新的依赖，能打开 JPG、PNG、WebP、GIF、BMP、ICO、AVIF，打不开 HEIC 和 TIFF；只能存成 JPG、PNG、WebP）；后端只负责把结果存进用户选的文件夹。原图从不改动。重新编码不带原图的拍摄信息；「压完反而更大时存原图」的那几张原样复制。

开机启动项（`crates/medkit-core/src/startup.rs`）：

- **列出**：脚本 `scripts/startup/list.ps1`（只读）列出登录用户的 Run、HKLM 的 Run 和它的 32 位路径、登录用户和所有用户的「启动」文件夹，给出每一项实际启动的程序（快捷方式会解析，rundll32、wscript 这类宿主程序取它运行的文件）、文件说明、公司和数字签名。路径只在本机界面显示，不进诊断报告。
- **开关**：和任务管理器「启动应用」是同一个：`HKCU` 或 `HKLM` 的 `Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`、`Run32`、`StartupFolder` 下和启动项同名的二进制值。第一个字节是单数（03，后面 8 个字节是停用的时间）表示停用；没有这个值、或者第一个字节是双数（02）表示启用。引擎自己读写这个值，只写这 5 个位置（写在代码里），只停用、不删除启动项本身，所以卸载小药箱以后在任务管理器里也能改回来。
- **修改日志**：每次开关记一条（功能 ID 是 `startup`），撤销时核对这个值有没有被别人（比如任务管理器）改过。以前的版本用删除 Run 值来停用（功能 ID `boot.startup-disable`），那些记录照样能撤销。
- **建议**：杀毒软件、输入法、Windows 自带的组件、硬件驱动和电脑厂商的功能标「建议保留」，其他的「不需要一开机就用的话，可以停用」；不说「建议停用」（原则 7）。
- 应用商店的应用有自己的开关（`ms-settings:startupapps`），任务计划不在这里管。

右键菜单（`crates/medkit-core/src/context_menu.rs`）：

- **列出**：脚本 `scripts/shell/context-menu-list.ps1`（只读）查 `Software\Classes` 下的 `*`、`AllFilesystemObjects`、`Directory`、`Folder`、`Directory\Background`、`DesktopBackground`、`Drive`，HKLM 和登录用户的都查：`shell\<名字>`（菜单命令）、`shellex\ContextMenuHandlers\<名字>`（外壳扩展，靠 CLSID 找到 DLL），再加上应用清单里的 `windows.fileExplorerContextMenus`（Windows 11 新菜单里应用加的项目）。找程序、读文件说明和签名的代码和开机启动项共用（shared block `program-info`）。菜单上的字是 `@文件,-编号`、`ms-resource:` 这类间接字符串的，引擎用 SHLoadIndirectString 解开（按资源读，不运行文件里的代码）。
- **只列第三方的**：程序在 Windows 目录里、微软签名的命令和外壳扩展、系统包，以及「打开方式」「发送到」「以前的版本」、Defender 扫描这些写死的 CLSID 都不列；对不上程序的命令、类里没写 DLL 的扩展也不列（不知道是谁加的）。CLSID 都没登记的外壳扩展（卸载后留下的空壳）照样列出来，说明拿掉没有坏处。应用商店里微软的应用（终端等）算应用，列出来。
- **开关**：菜单命令写空的 `ProgrammaticAccessOnly`（微软文档：菜单里不显示、程序照样能调用），写在它登记的那一侧（HKLM 或登录用户的 HKCU），下次右键生效；外壳扩展和应用的项目按 CLSID 写进 `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked`（空字符串值，名字是 CLSID），同一个 CLSID 在几个范围里登记的合成一项，重启资源管理器以后生效。恢复时把能让它不显示的值都删掉（也包括别的工具写的 `LegacyDisable` 和 HKCU 下的 Blocked）。只写这几个位置（写在代码里），只拿掉、不删除登记。
- **修改日志**：功能 ID 是 `context-menu`，标题用项目的名字（程序重启以后没有列表时，扩展用它登记的类名，命令用键名），状态说「显示 / 不显示（已拿掉）」。多个值的改动当成一个整体：中途失败，前面改过的按倒序退回（和数据文件里的功能共用一段代码）。

资源管理器里多出来的图标（`crates/medkit-core/src/shell_places.rs`）：

- **列出**：脚本 `scripts/shell/shell-places-list.ps1`（只读）查 HKLM 和登录用户的 `Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{CLSID}`（导航栏最上面一层，网盘、OneDrive 在这里）和 `…\MyComputer\NameSpace\{CLSID}`（「此电脑」里，WPS 云文档、百度网盘在这里），再看每个 CLSID 键：用户的和机器的那份在不在、`System.IsPinnedToNameSpaceTree` 是多少，程序（InProcServer32）在不在 Windows 目录里，指不指向一个文件夹（`Instance\InitPropertyBag` 里的 `TargetFolderPath`，网盘的做法）。同一个 CLSID 在两处、在机器的和用户的注册表里登记的算一项。
- **只列软件加的**：此电脑、网络、回收站、主文件夹、快速访问、库、控制面板、用户文件夹这些写死的 CLSID 不列；程序在 Windows 目录里、又不指向文件夹的（Windows 自己实现的）不列；OneDrive、图库、3D 对象、Linux 这几个 Windows 自带的列出来，标「Windows 自带」。没有 CLSID 键的（显示不出来）不列；只在导航栏里登记、又没有 `System.IsPinnedToNameSpaceTree` 的（本来就不在导航栏里显示）不列。
- **导航栏的开关**：资源管理器读合并视图 HKEY_CLASSES_ROOT：用户的 `Software\Classes\CLSID\{CLSID}` 里有这个值就用它的，没有用机器的（一个值一个值地合并，Windows 上的测试里核对过）；1 显示、0 不显示（微软《Integrate a Cloud Storage Provider》）。只在导航栏里的图标，隐藏时在用户那份里写 0（没有这份键就新建一个，和资源管理器「显示库」选项的做法一样），只影响当前用户；恢复时用户那份是 0、机器那份是显示的，就删掉用户的值（键空了一起删），回到软件自己登记的样子，不然写 1；只有机器那份、是 0 的（别的工具在所有用户的设置里隐藏的），写在机器那份里（界面上说明恢复以后所有用户都能看到）。32 位程序（它们的打开、保存对话框）看的 `WOW6432Node\CLSID` 那一份有这个值的，一样改。
- **「此电脑」里的**：隐藏时在登录用户的 `Software\Microsoft\Windows\CurrentVersion\Policies\NonEnum` 里写一个名字是 `{CLSID}`、值是 1 的 DWORD：外壳哪里都不列它（「此电脑」、导航栏、桌面、打开和保存对话框），组策略「删除桌面上的回收站图标」就是这么做的（微软《ADMX_Desktop Policy CSP》）。网上常说的 `HideMyComputerIcons` 没用：Windows 上的测试里外壳照样列出来。恢复时删掉这个值（所有用户的 NonEnum 里有的也删）。
- **修改日志**：功能 ID 是 `shell-places`，标题用图标的名字（程序重启以后没有列表时，用 CLSID 键里登记的名字），状态说「显示 / 不显示（已隐藏）」，用户那份里原来没有这个值的说「没有单独设置」。一次开关的几处改动当成一个整体，中途失败按倒序退回。Windows 上的测试照网盘的做法登记三个测试用的图标，另起进程用 Shell.Application 核对「此电脑」里还列不列、外壳读到的导航栏开关是多少。

改键（`crates/medkit-core/src/keymap.rs`）：

- **做法**：Windows 自带的扫描码映射（微软《Keyboard and mouse class drivers》的「Scan code mapper for keyboards」）：`HKLM\SYSTEM\CurrentControlSet\Control\Keyboard Layout` 的 `Scancode Map`（REG_BINARY）。开头 8 个字节是 0，接着 4 个字节是条数（算上最后的结束标记），每条 4 个字节（低 16 位是按下以后变成的扫描码、高 16 位是实际按下的键，变成 0 是不起作用），最后 4 个字节的 0。对所有账户、所有键盘有效，重启以后生效；不装驱动、不在后台运行。
- **能选的键**：写在代码里（主键区、功能键、编辑键、方向键、小键盘，扩展键写成 0xE0xx），名字和浏览器 `KeyboardEvent.code` 一样，界面上的键盘测试用的也是这套名字；音量、播放这几个多媒体键只能当「变成」的键。Fn 键和不少厂商自己的功能键（调亮度、开关触摸板这些）是键盘或者厂商的驱动自己处理的，不经过扫描码映射，改不了。
- **别的软件设的**：`Scancode Map` 格式不对、类型不是 REG_BINARY、或者里面有名单外的键（别的改键工具设的）时，界面只列认得出来的那几条，只给「全部恢复」（删掉整个值），不在上面接着改。
- **修改日志**：功能 ID 是 `key-remap`，标题是「键位重映射（改键）」，状态说成「Caps Lock（大写锁定） → 左 Ctrl；左 Win → 不起作用」这样，没有这个值说「没有改键（Windows 默认）」；撤销把原来的字节原样写回（原来没有就删掉），也是重启以后生效。Windows 上的测试写真的注册表，另起 PowerShell 核对类型和字节，再撤销。

文件删不掉：是谁占着（`crates/medkit-core/src/lockers.rs`、`crates/medkit-core/src/platform/restart_manager.rs`；工具箱里有，症状「删不掉、改不了名：提示『文件已在另一程序中打开』」的页面上也放了同一张卡片）：

- **查**：用 Windows 的重启管理器（Restart Manager，安装程序替换文件之前就用它找在用这些文件的程序）：把文件登记进一个会话，`RmGetList` 列出打开着这些文件、或者把它们当作程序模块加载了的进程。只查，不调用 `RmShutdown`：不关程序、不动文件，也不记修改日志。进程号和启动时间都对上才算同一个进程（进程已经退出、进程号被别人用上的不列）。
- **范围**：选了几个文件时一个一个查（最多 100 个），说得出哪个程序在用哪个文件；选了文件夹时把里面的文件（最多 5000 个，不跟着符号链接和目录联接走到外面）一起查一次，有人在用、文件又不超过 100 个时再一个一个查出是哪几个。
- **说法**：按重启管理器报的类型（有窗口的程序、命令行、资源管理器、后台程序、系统服务、关键进程）说怎么让它放手（`app/src/utils/fileLocks.ts`）；资源管理器在用时直接给「重启资源管理器」。查不出来的情况（文件夹本身被占着，比如命令行窗口停在里面——重启管理器只收文件；没有权限）在没查到时一并说明。
- **隐私**：要查的文件只能在后端的系统选择框里选，界面传不了路径；结果里只有文件名（文件夹里的用相对路径）和程序的文件名，没有完整路径（里面常有用户名），也不进诊断报告。

弹窗是哪个软件的（`crates/medkit-core/src/window_owner.rs`、`crates/medkit-core/src/platform/window_info.rs`、`app/src/components/PopupOwner.vue`）：

- **找窗口**：倒数几秒（让用户把鼠标移到弹窗上），`GetCursorPos` + `WindowFromPoint`，再用 `GetAncestor(GA_ROOT)` 找到最外层的窗口：网页、广告内容常在别的进程（浏览器内核、WebView2）的子窗口里，最外层的窗口才是弹出它的那个程序的。看的是鼠标下面的窗口，不是激活的窗口：有的广告弹窗点了也不会被激活。只读：不关窗口、不结束程序，也不记修改日志。
- **认程序**：`OpenProcess`（只查询）+ `QueryFullProcessImageNameW` 得到程序文件；版本信息（`GetFileVersionInfoW`、`VerQueryValueW`，优先简体中文那一份）给说明、公司、产品名，只读资源，不加载、不运行这个程序。任务栏、桌面按窗口类名认；`ShellExperienceHost.exe` 画的是 Windows 通知（别的软件、网站发的通知也是它显示的），Windows 文件夹里的算 Windows 自带的；小药箱自己的窗口单独说。
- **属于哪个软件**：按程序所在的文件夹对上「应用和功能」里的软件（HKLM 的 64 位、32 位卸载信息，加上登录用户自己的；系统组件、补丁不算）：安装位置、图标、卸载程序所在的文件夹里包含这个程序，有好几个时取最具体的那个。Program Files、ProgramData、用户的 AppData、下载、桌面、Windows 文件夹这些太宽的文件夹不拿来认。
- **路径**：结果里的文件夹把用户文件夹名换成 `*`；完整路径只留在后端，「打开所在的文件夹」不收界面传来的路径。结果不进诊断报告。

此应用无法在你的电脑上运行（`crates/medkit-core/src/exe_info.rs`、`src-tauri/src/exe_check.rs`、`app/src/components/ExeCheck.vue`、`app/src/utils/exeCheck.ts`，放在同名症状的页面上）：

- **读文件头**：照微软《[PE Format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format)》：`MZ` 头里 0x3C 处的偏移找到新头；`PE\0\0` 的读 COFF 头的 Machine、Characteristics（DLL 位）、可选头的 Subsystem 和数据目录（第 14 项有值是 .NET 程序），再用节表里每一节在文件里的结束位置和证书表（第 4 项，按文件偏移）算文件至少多长，比实际短就是没下载完整；`NE`、`LE`、`LX` 或者只有 DOS 头的是 16 位老程序；不是 `MZ` 开头的按开头的签名猜是 MSI（OLE 复合文档）、压缩包、PDF 还是网页。只读，不运行、不加载它。
- **和这台电脑对**：x86 的哪都能跑；x64 的要 x64，或者 Windows 11 的 ARM 电脑（Windows 10 的 ARM 电脑只能模拟 x86）；ARM64 的只能在 ARM 电脑上；32 位 ARM 的只在 24H2 以前的 Windows 11 ARM 电脑上（微软《Update app architecture from Arm32 to Arm64》说新版 Windows 11 不再支持，第三方报道是 24H2 起）；安腾的都不行；子系统不是图形界面、命令行的（驱动、EFI 这些）不是桌面程序。16 位程序：微软说 64 位和 ARM 版的 Windows 没有 NTVDM。
- **说法**在界面（`exeCheck.ts`，node 测试）：该下载哪个版本、怎么看自己的电脑、命令行程序双击黑框一闪是正常的；文件本身没问题的，给兼容模式和「程序兼容性疑难解答」。Windows 上的测试拿系统自带的记事本（64 位和 SysWOW64 里 32 位的）、cmd、kernel32.dll 和截掉后半截的记事本核对。

回收站坏了（`crates/medkit-core/src/recycle_bin.rs`、`src-tauri/src/recycle_bin.rs`、`app/src/components/RecycleBinRepair.vue`，放在症状「提示『回收站已损坏』」的页面上）：

- **办法**：照微软《[The Recycle Bin is corrupted](https://learn.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/recycle-bin-corrupted)》：提示「X:\ 上的回收站已损坏」时，删掉那个盘根目录下的 `$Recycle.Bin` 文件夹（原文是在管理员命令提示符里运行 `RD <盘>\$Recycle.bin /s /q`），再重启，Windows 会重新建一个。症状的手动步骤先让用户点提示框里的「是」，一再提示才用这个。
- **先数再删**：列出各个盘的回收站里有多少个文件、多大，确认框里说清楚：这台电脑上所有账户放进这个盘回收站的都会删掉、找不回来，要留的先从回收站里还原。结果和确认框里只有个数和大小。
- **只删这一个位置**：盘符只能是一个字母、而且是现在还在的盘；删的就是 `<盘的根目录>\$Recycle.Bin`。它是链接（符号链接、目录联接）时只删链接本身；是文件夹时用 `remove_dir_all`（里面的链接也只删链接，不跟着走到别处）。NTFS 上只读、系统、隐藏属性不挡删除；删不掉时把只读都去掉再删一次（FAT32、exFAT 的盘上要这样）。还是删不干净的（文件正被别的程序用着）再数一次还剩多少，让用户重启以后再来。
- **不记修改日志**：删掉的东西没法撤销，和「清空打印队列」「重建图标和缩略图缓存」这些小工具一样不进修改日志，确认框就是最后一道。Windows 上的测试在临时文件夹里摆一个带只读、隐藏、系统属性和目录联接的 `$Recycle.Bin`，核对数得对、删得干净、联接指向的地方不动；不碰真的盘上的回收站。

找大文件和重复文件（`src-tauri/src/space.rs`、`app/src/components/SpaceFinder.vue`）：

- **只读**：不删、不改、不移动文件。要删的话，点「显示」（SHOpenFolderAndSelectItems：打开所在的文件夹并选中它，不打开文件），用户自己在资源管理器里删，删掉的先进回收站。
- **数**：不跟着符号链接、目录联接走；「仅在线」的网盘文件（属性 Offline、RecallOnOpen、RecallOnDataAccess）不占本机空间，读它还会从网上下载，不算。最多数 60 秒、50 万个文件；列出最大的 50 个。
- **重复**：只比 1 MB 以上的；大小一样 → 开头 64 KB 一样 → 全部内容逐字节比较，完全一样才算（不靠哈希猜，也不用加依赖）。大的先比，最多比 60 秒。Windows 文件夹、Program Files、ProgramData 里的和带「系统」属性的文件标成「系统或程序的文件，别手动删」，不参与找重复（系统文件夹里大量硬链接，看着一样，删了也腾不出地方）。
- **路径**：文件夹只能在后端的系统选择框里选；界面拿到的是相对这个文件夹的路径，「显示」只传结果里的编号。结果不进诊断报告。

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
export interface SymptomDetail extends SymptomSummary { causes: string[]; guide: string | null; steps: SymptomStep[]; links: string[] }
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
export interface ToolRow { label: string; value: string; secret: boolean; qr: boolean }   // secret：默认遮住，不进「复制全部」；qr：值画成二维码（扫码连 WiFi，也是 secret）
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

四选一：

```yaml
open: { program: device-manager }             # 系统工具：名字必须在下面的名单里
open: { settings: windowsupdate }             # 「设置」里的一页（ms-settings:<页面>）：页面也必须在名单里
open: { troubleshooter: AudioTroubleshooter } # 「获取帮助」里微软的疑难解答（ms-contact-support://smc-to-emerald/<名字>）
open: { website: oem-drivers }                # 网页：只能写名单里的名字，网址由引擎按这台电脑挑
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
| `uac-settings` | `UserAccountControlSettings.exe` |
| `firewall` | `control.exe firewall.cpl` |
| `indexing-options` | `control.exe srchadmin.dll` |
| `windows-features` | `OptionalFeatures.exe` |
| `advanced-sharing` | `control.exe /name Microsoft.NetworkAndSharingCenter /page Advanced`（「高级共享设置」；Win11 22H2 起转到「设置」里的同一页） |
| `firewall-advanced` | `mmc.exe wf.msc`（高级安全 Windows Defender 防火墙） |
| `region` | `control.exe intl.cpl`（控制面板的「区域」，「管理」页里改「非 Unicode 程序的语言」） |
| `network-connections` | `control.exe netconnections`（控制面板的「网络连接」：网卡的禁用、启用和属性） |
| `system-protection` | `SystemPropertiesProtection.exe`（「系统属性」的「系统保护」页：开关系统保护、创建还原点） |
| `environment-variables` | `SystemPropertiesAdvanced.exe`（「系统属性」的「高级」页：下面的「环境变量」按钮改 Path、PATHEXT） |
| `task-scheduler` | `mmc.exe taskschd.msc`（「任务计划程序」：禁用、启用计划任务） |
| `cleartype` | `cttune.exe`（「ClearType 文本调谐器」：控制面板里「调整 ClearType 文本」打开的就是它） |
| `store-reset` | `WSReset.exe`（重置 Microsoft Store 的缓存：空白窗口自己关掉、商店自动打开，不删应用） |
| `system-file-repair` | 新的命令行窗口：`cmd.exe /k ""<System32>\Dism.exe" /Online /Cleanup-Image /RestoreHealth & "<System32>\sfc.exe" /scannow"`（`ShellExecute`，当前文件夹是 System32；窗口留着看结果） |

settings 页面：`windowsupdate`、`windowsupdate-optionalupdates`、`windowsupdate-history`、`windowsupdate-activehours`、`storagesense`、`storagepolicies`、`appsfeatures`、`startupapps`、`defaultapps`、`network-status`、`printers`、
`sound`、`powersleep`、`batterysaver-usagedetails`、`display`、`display-advancedgraphics`、`bluetooth`、`recovery`、`windowsdefender`、`privacy-microphone`、`privacy-webcam`、`dateandtime`、`easeofaccess-keyboard`、`easeofaccess-mouse`、`easeofaccess-colorfilter`、`easeofaccess-highcontrast`、`nightlight`、`regionlanguage`、`apps-volume`、`notifications`、`taskbar`、`signinoptions`、`lockscreen`、`network-mobilehotspot`、`devices-touchpad`、`troubleshoot`，以及 5 个自带应用的「高级选项」（终止、修复、重置）：`appsfeatures-app?<包系列名>`，包系列名是 `Microsoft.Windows.Photos_8wekyb3d8bbwe`（照片）、`Microsoft.WindowsCalculator_8wekyb3d8bbwe`（计算器）、`Microsoft.WindowsCamera_8wekyb3d8bbwe`（相机）、`Microsoft.WindowsStore_8wekyb3d8bbwe`（Microsoft Store）、`Microsoft.ZuneMusic_8wekyb3d8bbwe`（媒体播放器，Windows 10 上叫 Groove 音乐），照微软《Keep removed apps from returning during an update》，整串写进名单。

troubleshooter（照微软《[Windows troubleshooters](https://support.microsoft.com/en-us/support/get-help/windows-troubleshooters)》列的 10 个，名字一字不差）：`AudioTroubleshooter`、`BITSTroubleshooter`、`BluetoothTroubleshooter`、`TroubleshootCamera`、`NetworkAndInternetTroubleshooter`、`PrinterTroubleshooter`、`ProgramCompatTroubleshooter`、`VideoPlaybackTroubleshooter`、`WMPTroubleshooter`、`WUTroubleshooter`。由「获取帮助」应用打开、检查和修复（要联网），小药箱只负责打开，不记修改日志；界面上按钮写「运行微软的「…」」，工具箱里单独一组。没有「获取帮助」应用的电脑（精简过的系统、服务器版）如实说明，让用户在 Microsoft Store 里装上，或者到「设置」的「疑难解答」页里找：先用 AssocQueryStringW 查有没有能打开 `ms-contact-support:` 链接的应用，没有就不去打开（不然系统会弹「需要使用新应用以打开此链接」，有的系统上 ShellExecuteExW 还会一直等着这个没人点的对话框）。打开「设置」、链接、文件夹的 ShellExecuteExW 都在单独的线程上调用，最多等 30 秒，按钮不会一直转圈。不用 `ms-msdt:` 协议（Follina 漏洞的入口），也不直接跑 msdt.exe：Windows 11 22H2 以后的版本已经把它退役了。

website（网址都写在 `tools.rs` 里，数据文件只能挑名字）：

- `oem-drivers`：电脑品牌官网的驱动下载页。引擎读 `HKLM\HARDWARE\DESCRIPTION\System\BIOS` 里的
  SystemManufacturer、SystemProductName、SystemVersion、SystemFamily、BaseBoardManufacturer、BaseBoardProduct、BIOSVendor
  （开机时系统从 SMBIOS 抄过来，这里没有序列号），按顺序认：虚拟机（照 systemd 的 `virt.c`：产品名、系统厂商、主板厂商、
  BIOS 厂商、产品版本里有一项以 `VMware`、`QEMU`、`innotek GmbH`、`Hyper-V` 这些开头；另外 Microsoft Corporation +
  Virtual Machine 是 Hyper-V）→ Surface（Microsoft Corporation + 产品名以 Surface 开头）→ 按系统厂商认品牌机 → 按主板
  厂商认自己组装的电脑（华硕、微星、技嘉、华擎）→ 认不出。厂商名只留字母数字、转大写以后比较，太短的名字（`HP`、`ASUS`、
  `MSI`、`TIMI`）要整串相等，HPE（慧与）的服务器不算惠普。各品牌的写法照 systemd 的 hwdb 和 Linux 内核里按 DMI 认
  笔记本的表。品牌机打开品牌官网，告诉用户在网页上搜哪个型号（联想的型号名在 SystemVersion、SystemFamily 里，产品名
  是机器类型编号）；自己组装的电脑打开主板品牌的下载页，给出主板型号；虚拟机、认不出的品牌不打开，说明原因（认不出时提醒
  别用搜索结果里的「驱动下载站」）。

  | 品牌 | 驱动下载页 |
  |---|---|
  | 联想 | https://newsupport.lenovo.com.cn/driveDownloads_index.html |
  | 惠普 | https://support.hp.com/cn-zh/drivers |
  | 戴尔（含外星人） | https://www.dell.com/support/home/zh-cn?app=drivers |
  | 华硕 | https://www.asus.com.cn/support/download-center/ |
  | 宏碁 | https://www.acer.com.cn/support.html?type=1 |
  | 华为 | https://consumer.huawei.com/cn/support/ |
  | 荣耀 | https://www.honor.com/cn/support/ |
  | 小米 | https://www.mi.com/service/notebook/drivers |
  | 微星 | https://cn.msi.com/service/download |
  | 三星 | https://www.samsung.com.cn/support/ |
  | 技嘉 | https://www.gigabyte.cn/Support/Consumer/Download |
  | 华擎 | https://www.asrock.com/support/index.cn.asp |
  | 微软 Surface | https://support.microsoft.com/zh-cn/surface/drivers-firmware/download-drivers-and-firmware-for-surface |

  机械革命、雷神、机械师、神舟这些国产游戏本多是同方、蓝天的模具，BIOS 里的厂商随经销商变，官网也核实不了，先不收。
- 网址固定的官方网页（`tools.rs` 的 `FIXED_WEBSITES`，名字也在 `WEBSITES` 里，都是逐个打开核实过内容的微软中文页面）：
  `office-uninstall`（《从电脑卸载 Microsoft 365 或 Office》，卸不掉、卸不干净时用页面里的「卸载支持工具」）、
  `office-install`（《在电脑或 Mac 上下载、安装或重新安装 Microsoft 365 或 Office 2024》，用买 Office 的账户登录下载）。
- 网页不由小药箱直接打开：小药箱是管理员，直接 ShellExecute 网址的话浏览器也会以管理员身份运行（下载的安装包不再弹
  UAC，已经开着的浏览器也接不上）。照微软的 ExecInExplorer 示例和 Raymond Chen《How can I launch an unelevated process
  from my elevated process and vice versa?》，请桌面的资源管理器（登录用户的普通权限）替我们打开：ShellWindows →
  桌面窗口 → 外壳视图 → Shell.Application（IShellDispatch2）→ ShellExecute，调用前照 Firefox 的做法
  CoAllowSetForegroundWindow，浏览器窗口才会到最前面（`platform/explorer_exec.rs`）。在单独的线程上调用，最多等 30 秒。
  找不到桌面（资源管理器没在运行）时不退回到直接打开，把网址告诉用户，让他自己在浏览器里打开。
- `tool_open` 返回打开以后要告诉用户的话（网页：打开的是哪个品牌的页面、在上面搜什么），没有时界面说「已经打开了」。
- 系统工具以小药箱的权限（管理员）启动，所以不会再弹一次 UAC；「设置」页面由系统打开。
- 命令行窗口里运行的程序都写 System32 下的绝对路径（cmd 找程序时先找当前文件夹，便携版可能放在「下载」里），
  一条失败了下一条照样运行（`&`）；整串外面再包一层引号，里面有引号和 `&` 时 cmd 只去掉最外面那一对。
- 精简系统上被删掉的工具，打开时如实说「这台电脑上没有这个工具」，不去别处找。

