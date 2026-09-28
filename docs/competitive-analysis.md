# 竞品调研与取长补短（2026 年 9 月）

> 配合[《执行计划书》](plan.md)使用，是对前一份《电脑维护工具箱_竞品调研与分析》的更新和补充。
>
> 逐项的功能对照、能直接复用的开源数据、国内高频问题和官方错误码，见后续的[《功能缺口与可复用资源调研》](feature-gap-research.md)（2026-09-28）。
>
> - 调研时间：2026-09-26。
> - 方法：分 5 类，逐款核对 2025–2026 年的现状。来源包括官网、GitHub 仓库和数据文件、微软文档、安全厂商报告、公开投诉。
> - 只找到单一来源或二手来源的内容，标注「（未核实）」。
> - 涉及厂商不当行为的内容，都注明了出处，多数来自火绒安全的公开报告。

---

## 一、结论先行

1. **空白还在。** 没有一款产品同时做到「按症状修复、每步可撤销、开源、中文、零推广」。
2. **国内管家的信任问题没有好转。**
   - 金山毒霸 2026 年被火绒通报：借下载站的安装包静默安装，还用内核驱动绕过其他安全软件的自我保护。
   - 鲁大师 2025 年被通报云控推广，还刻意躲避检测。
   - 360 有用户投诉被静默装上「全家桶」，点了「今日不再弹窗」，第二天照样弹。
   - 免费人工服务在收缩；付费的「DLL 修复」「远程修复」，有用户投诉修不好。
3. **微软在补短板，也在拆旧东西。**
   - 补：「获取帮助」里的自动疑难解答、「通过 Windows 更新重装当前系统」、时间点还原（2026 年 7 月正式推送）、快速机器恢复。
   - 拆：MSDT 疑难解答、wmic、SaRA、局域网的「PC 到 PC 迁移」。
   - 我们的对策：把系统能力编进修复阶梯，不依赖已经移除的组件。
4. **撤销的做法已经收敛到「运行时快照」。**
   - 已转向快照的：Win11Debloat（2026 年 5 月）、O&O ShutUp10++（2026 年 8 月）、Winhance。
   - 仍然写死「默认值」的：WinUtil、privacy.sexy、Sophia Script。撤销后回到的是系统默认，而不是用户原来的设置。
5. **大多数工具不检测当前状态。** 只有 Dism++ 的 State 段、WinUtil 的开关类有检测。检测应该是一等公民。
6. **高权限工具的漏洞，集中出在「本地入口加上校验不严」。**
   - Dell、ASUS、联想、MSI 的厂商管家，都在本地服务或 URL 协议上出过提权漏洞。
   - UniGetUI 2026 年的三个漏洞，都出在软件匹配和导入上。
7. **有现成的数据和做法可以复用**：Dism++ 的清理规则（MIT）、winapp2.ini（CC BY-SA 4.0）、Rufus 的应答选项、CrystalDiskInfo 的健康阈值、BCUninstaller 的残留评分规则。
8. **国内装软件的坑很具体。**
   - winget 里国产软件的 ID 经常变，还有停更的清单。
   - 镜像只加速索引，不加速安装包。
   - 微软商店在国内能用，也收录了不少国产软件。
   - 下载站的「高速下载器」捆绑依旧存在。
9. **AI 的实际情况。** 国内的 AI 主要用在办公问答，以及 2026 年兴起的「龙虾」防护（「龙虾」指 OpenClaw 等本地 AI 智能体）。真正用 AI 修电脑的很少。Win11 设置里的 AI 代理，地区限制写明不对中国开放。
10. **单人维护的项目容易停更。** privacy.sexy 停滞，Dism++ 停更，optimizer 归档，MediaCreationTool.bat 停更，WhyNotWin11 两年没发新版。

---

## 二、竞品地图

| 类别 | 代表产品 | 和我们的关系 |
|---|---|---|
| 国内综合管家 | 360 安全卫士、腾讯电脑管家、金山毒霸、火绒、微软电脑管家、软媒魔方；厂商管家（联想、华为、荣耀、小米） | 小白的默认选择，最直接的竞争对手 |
| 症状修复 / 急救 | 360 断网急救箱和系统急救箱、腾讯电脑诊所、金山电脑医生、FixWin、Tweaking.com Windows Repair、NetAdapter Repair、Complete Internet Repair、wureset | 核心模块的直接对标 |
| 微软系统自带 | 获取帮助、重装当前版本、时间点还原、快速机器恢复、PC 健康检查、Windows 备份、设置里的 AI 代理 | 既是竞争者，也是我们修复阶梯里的一级 |
| 开源设置 / 优化 / 清理 | WinUtil、Win11Debloat、Sophia Script、privacy.sexy、O&O ShutUp10++、Winaero Tweaker、optimizer、Winhance、Dism++、BleachBit、winapp2.ini | 数据模型和撤销机制的参照 |
| 装软件 / 卸载 / 磁盘分析 | Ninite、UniGetUI、winget、Scoop、Chocolatey、Patch My PC、BCUninstaller、Revo、Geek、HiBit、WizTree、WinDirStat、SpaceSniffer、TreeSize | 常用功能模块的参照 |
| 重装 / 启动盘 / 硬件 / 驱动 | Rufus、Ventoy、微 PE、Edgeless、FirPE、优启通、HotPE、unattend-generator、图吧工具箱、HWiNFO、CrystalDiskInfo、SDIO、DDU、驱动总裁、一键重装工具 | 重装助手和验机的参照，也有不少反面教材 |

---

## 三、分类详述

### 3.1 国内综合管家与急救工具

**360 安全卫士**（免费，靠广告推广和会员赚钱；仍在活跃更新）

- **断网急救箱**检测 7 项：
  - 网络硬件配置（网线、网卡、驱动）
  - 网络连接配置（IP）
  - DHCP
  - DNS
  - HOSTS
  - 浏览器配置（代理、VPN、插件）
  - LSP（加速器常见）

  修复分普通、强力两档。官方还专门出了**离线独立版**，理由是断网时用户没法上网找办法。
- **系统急救箱** 5.1（2026-07）：需要重启多次，重启后流程能接着跑。它的 DLL 恢复是从 360 云端下载 DLL。
- **已下线**：人工服务和蓝屏修复器已于 2024-04-30 下线。据第三方介绍，蓝屏修复器是把 dump 文件上传到云端分析的。
- **借鉴**：
  - 检测清单
  - 修复分轻重
  - 网络工具离线可用
  - 需要多次重启的流程能接着跑
- **避免**：
  - 弹窗设置有 50 多项，却没有一键全关。据 2025-03 的报道，点了「今日不再弹窗」，第二天照样弹。
  - 2024-06 有用户投诉，被静默装上「全家桶」。
  - 火绒 2025-07 的报告：360 浏览器推广平台分发的软件带毒。
  - 「开机击败全国 x%」和体检打分。
  - 从云端拼凑 DLL。

**腾讯电脑管家**（免费，导流腾讯生态；18.0，2026-03）

- **电脑诊所**：
  - 分类：常见问题、上网异常、软件硬件、系统综合等，顶部有搜索框。
  - 每个条目按「标题 / 主要症状 / 可能原因 / 修复步骤」组织。用户先看症状和原因，再点「立即修复」。
  - 页面原话写明，会「优先选择『最有用』的攻略开发为一键修复」。
  - 有「提交问题」入口。
- **软件管家**：称每款软件都经过人工测试；「一键装机」可以批量勾选。
- **借鉴**：
  - 症状卡片的结构
  - 图文攻略验证有效后，再升级为一键修复
  - 人工审核的装机清单
- **避免**：
  - 问题库里大量条目还停在 Flash、IE 时代
  - 腾讯专区导流
  - AI 助手常驻在悬浮球和屏幕边缘

**金山毒霸**（杀毒免费，另有会员；运营方是珠海海鸟科技，驱动精灵也归它运营）

- **电脑医生**：按小白能懂的症状进入，包括卡慢、C 盘满、DLL 缺失、打印机、没声音、网络、显示、软件闪退、数据恢复。付费会员还包含远程人工服务（价格未核实）。
- **避免**：
  - 「免费诊断，付费修复」：2024-08 的黑猫投诉中，用户付了 30 元，DLL 问题仍然没修好。
  - 火绒 2026-05-29 的报告：通过下载站安装包里隐蔽的推广框架静默安装；用带金山签名的内核驱动，绕过了 6 款安全软件的自我保护。
  - 火绒 2025-07 的报告：病毒捆绑在 ToDesk 等软件的推广安装包里，会静默安装毒霸。
  - 大量 SEO 页面给自家产品导流。

**火绒安全**（口碑最好，官网承诺没有广告弹窗、不捆绑；6.0 系列）

- **弹窗拦截**：
  - 自动检测，一键拦截
  - 「截图拦截」可以选择关闭或隐藏
  - 「窗口记录」可以补拦漏掉的弹窗
  - 每条规则都能单独开关
- **启动项管理**：扫描注册表、服务、计划任务三类，给出「维持现状 / 建议禁止 / 可以禁止」三级建议。**「操作记录」对比「原始状态」和「操作结果」，可以一键「恢复原始状态」。**
- **断网修复**检测 8 项：网络硬件配置、网络连接配置、DHCP、DNS、hosts、LSP、IE 代理、环境变量。
- **诊断报告**：安全分析工具从 2024-11 起可以导出系统诊断报告。
- **不足**：
  - 论坛有用户反馈，断网修复「检测不出问题」「DNS 修不好」。
  - 2025-12 火绒发了预警：有仿冒官网排在搜索结果第二位，或者占了广告位。
- **借鉴**：
  - **「操作记录 + 恢复原状」是「每步可撤销」最好的界面范本**
  - 三级建议
  - 诊断报告导出

**微软电脑管家**（微软中国团队开发，免费；3.22.4，2026-08）

- **功能**：
  - 体检、一键加速、存储管理、启动项和进程管理
  - 弹窗管理：识别弹窗来自哪个应用，支持截图拦截
  - Windows 功能修复：可以重置默认应用；任务栏修复会关掉第三方插件，但保留用户自己的任务栏设置
  - AI 圈选
  - 「龙虾管理」：一键停止或卸载 OpenClaw 等 AI 智能体，并提醒用户去第三方平台撤销授权
- **争议**：
  - 2024-05，它把「Edge 改用了别的搜索引擎」列为修复建议，劝用户改回 Bing。
  - 2024-06 起，它随系统更新，自动安装到区域设为中国的 Win11 上。
  - 据外媒 2026-07 的报道，新的 Restore 面板仍然把换掉 Bing、改任务栏样式标记为异常。
- **口碑**：新浪 2025-07 的评测认为它干净，但安全防护只是跳转到 Windows 安全中心，也缺少系统修复和驱动更新。
- **借鉴**：
  - 识别弹窗来源
  - 修复时保留用户自己的设置
- **避免**：把用户自己的选择当成问题。

**软媒魔方**（IT之家母公司软媒出品）：设置大师把藏得深的系统设置集中起来，逐项解释。官网最新仍是 2020-08 的 6.25，实际已经停更。

**厂商管家**

- **联想电脑管家**：原厂驱动、电池养护、C 盘应用迁移、右键菜单管理、「异常报错弹窗处理」（2026-02）、管家智能体（2026-04）。2019 年底起有用户指责，它的「浏览器保护」功能把主页锁成了导航站。
- **华为电脑管家**：
  - 故障排查分一键检测和单项检测（连接、音频、卡顿与死机、系统软件）。
  - 智能充电的提示框只给「继续保护 / 恢复充电」两个选项。
  - 远程诊断时，用户输入工程师给的连接码，**可以只共享部分窗口**。
- **荣耀**：YOYO 助理接入了 DeepSeek，称能「Windows 系统智控」，但具体能控制什么没有公开。
- **小米**：以跨屏互联为主，维护功能很少。
- **借鉴**：
  - 按部件做单项检测
  - 提示只给两个明确的选项
  - 只共享部分窗口的隐私设计

**鲁大师、驱动精灵、驱动人生**（反面参照）

- **鲁大师**：
  - 2024-08 的黑猫投诉：用户付 38 元买了 DLL 修复，没修好，申请退款被拒。
  - 火绒 2025-11-11 的报告：鲁大师及其关联企业云控推广，包括弹窗推页游、擅自装软件、篡改京东链接插入返佣参数。它还刻意躲避检测：避开北京 IP、识别技术人员和虚拟机、最长延迟 180 天才触发。
- **驱动精灵**：火绒 2019-12 的报告称，它在卸载时投放后门、锁定浏览器首页。
- **驱动人生**：2018 年升级服务器被劫持，2 小时内约 10 万台电脑感染。
- **唯一值得看的**：两家都有「网卡版」，正视了「没有网卡驱动就上不了网，上不了网也就下不了驱动」这个死结。

### 3.2 国际修复工具与微软自带能力

**FixWin 11**（免费便携，闭源；11.2，2025）

- **做法**：修复项分 6 类，每项旁边有「?」按钮说明它做什么；可以复制底层命令；提示「一次只修一项，重启后确认」。
- **不足**：
  - 没有逐项撤销，只能靠还原点。
  - 按组件分类而不是按症状分类，小白不知道该选哪一项。

**Tweaking.com Windows Repair**（修复免费，Pro 版收费；更新日志停在 2023 年）

- **借鉴**：
  - 修复前按步骤强制预检：先查毒 → chkdsk → SFC → 建还原点并备份注册表。
  - 提供「一键进安全模式，修完自动退出」的机制。
- **反面**：它按内置的「默认权限」数据批量重置权限，而这份数据最后更新于 2022-09。系统版本一变，就可能把系统改坏。

**NetAdapter Repair、Complete Internet Repair**

- **NetAdapter Repair**（开源，2015 年之后停更）
  - 借鉴：日志记录每一项是成功还是失败；清空 hosts 前，先把原内容写进日志。
  - 反面：写死了「改用 Google DNS」，在国内不通；会向第三方网站查询公网 IP。
- **Complete Internet Repair**
  - 首页写明「不要修没坏的组件，看不懂就跳过」。
  - 清空目录前先备份，备份失败就不删。
  - 用 C++ 重写的新仓库还没有声明许可证。

**wureset**（重置 Windows 更新；批处理脚本，MIT；原仓库 2026-01 归档）

- **借鉴**：
  - 服务没停下来就中止。
  - 缓存目录改名，不删除。
- **避免**：
  - 默认就执行改名和权限重置。微软的说法是，这两步要在其他办法都无效后才做。
  - 它的「删除错误的注册表值」会删掉整个 `Policies\WindowsUpdate`，单位下发的更新策略也会被一起清掉。
  - 仍然依赖已被移除的 wmic。

**微软「获取帮助」与 MSDT 退役**

- **现在的疑难解答**：Win11 的「获取帮助」里有 10 个自动疑难解答，分别是音频、BITS、蓝牙、相机、网络、打印机、程序兼容性、视频播放、WMP、Windows 更新。可以用 `ms-contact-support://smc-to-emerald/<名称>` 直接拉起。
- **MSDT 分三年退役**：2023 年重定向到「获取帮助」，2024 年移除一批，2025 年移除 MSDT 平台本身。只影响 22H2 之后的 Win11。
- **教训**：Follina 漏洞（CVE-2022-30190）就是通过 ms-msdt 协议利用的。「排障工具 + URL 协议」本身就是攻击面。

**微软的恢复能力**

- **快速机器恢复**：24H2 起提供，家庭版默认开启。
- **「使用 Windows 更新修复问题」**（立即重新安装）：重装当前版本，保留应用、文件和设置。需要联网；单位管理的电脑上可能没有这个选项。
- **时间点还原**：2026-07 正式推送。每 24 小时一份快照，保留 72 小时，家庭版默认开启。**还原时，本地用户文件会一起回滚。**
- **微软推荐的顺序**：疑难解答 → 重装当前版本 → 卸载更新 → 时间点还原 → 重置此电脑。
- **已移除的组件**：SaRA 命令行工具从 2026-03-10 起从 Windows 中移除；wmic 也已经从现役 Win11 中移除。

**Windows 备份与 PC 到 PC 迁移**

- **Windows 备份**：只支持个人微软账户，文件放进 OneDrive（免费 5GB），应用和设置只能在新电脑的开机设置阶段恢复。
- **局域网「PC 到 PC 迁移」**：本来就不迁移应用和密码。2026-09 的预览版起，这个功能已不再提供。
- **结论**：不绑账户、不依赖云的本地迁移，正好是一块空白。

**PC 健康检查与 WhyNotWin11**

- **PC 健康检查**：逐项写明哪一项不满足，并附操作指引；同时也在推广 OneDrive。
- **WhyNotWin11**（LGPL-3.0，2024-10 之后没有新版本）：做 11 项检查，每一项都写清「这是什么、能不能修、怎么修」。例如：「在 BIOS 里开启」「用 MBR2GPT 转换」「台式机可以换 CPU，笔记本不能」。

**Autoruns 与 AdwCleaner**

- **Autoruns**：
  - 要先开启「验证代码签名」，再用「隐藏微软项」。否则它只看文件里的「Microsoft Corporation」字样，而这个字样是可以伪造的。
  - 取消勾选只是禁用，可以恢复；删除才是永久的。
  - 开启「提交未知文件」后，会把整个文件上传到 VirusTotal。
- **AdwCleaner**：
  - 按服务、文件、快捷方式、计划任务、注册表、浏览器、hosts、预装软件分类清理。
  - 删除的项目全部进隔离区，可以恢复。

**BlueScreenView 与 WhoCrashed**

- **BlueScreenView**：不需要调试符号。它把崩溃栈里的地址和已加载模块的地址范围比对，标出「可能是哪个驱动」，并且说明结果不是 100% 准确。
- **WhoCrashed**：用人话写结论，也声明无法总是确定根因。
- **借鉴**：输出「嫌疑排序 + 把握大小 + 免责声明」，不下「就是它」的结论。

**厂商管家的漏洞史**（安全教训）

- **Dell SupportAssist（2019）**：本地 HTTP 服务只校验来源域名的后缀。配合 DNS 欺骗，网页就能让它以管理员身份下载并运行文件。
- **ASUS DriverHub（2025）**：本地 RPC 的来源检查只判断「是否包含」官方域名。访问一个网页，就能静默执行代码。
- **Lenovo Vantage（2025）**：SYSTEM 服务靠「调用方有联想签名」来认证，被「复制一个签名程序再加 DLL 劫持」绕过。另外，注册表白名单按子串匹配，校验和读取之间还有竞态问题。
- **MSI Center（2025）**：本地端口直连 SYSTEM 服务，签名校验和执行之间可以偷换文件。
- **共同成因**：高权限组件，加上本地通信或 URL 协议入口，再加上边界校验不严。

### 3.3 开源设置 / 优化 / 清理工具：数据模型与撤销

| 项目 | 许可证 | Star（2026-09） | 维护状态 |
|---|---|---|---|
| WinUtil | MIT | 6.3 万 | 活跃 |
| Win11Debloat | MIT | 5.8 万 | 活跃，2026 年起有图形界面 |
| Sophia Script | MIT | 9.8k | 活跃，有简体中文 |
| privacy.sexy | AGPL-3.0 | 6.1k | 2025-04 之后没有提交 |
| O&O ShutUp10++ | 闭源免费 | — | 活跃 |
| Winaero Tweaker | 闭源免费 | — | 活跃 |
| optimizer | GPL-3.0 | 1.8 万 | **2026-01 已归档**，由 optimizerNXT 接替 |
| Winhance | **PolyForm Shield**（不是开源许可证） | 1.3 万 | 活跃 |
| BleachBit | GPL-3.0+ | 7.0k | 活跃 |
| winapp2.ini | CC BY-SA 4.0 | 1.0k | 活跃 |
| Dism++ | 核心闭源；规则仓库 MIT | 2.0 万 | 实际已停更 |

**撤销机制对比**

| 项目 | 数据格式 | 撤销方式 | 推荐等级 | 导出导入 | 命令行 |
|---|---|---|---|---|---|
| WinUtil | JSON（registry / service / 脚本） | 写死在 JSON 里的 `Original*` 值 | Standard / Minimal / Advanced 三档预设 | 导出所选 ID 的 JSON | `-Config`、`-Preset` |
| Win11Debloat | Features.json + .reg | **2026-05 起改为运行时快照**；可选建还原点 | 默认 / 精简默认 | JSON，导入前先预览 | `-RunDefaults`、`-Silent`、`-Sysprep`、`-User` |
| Sophia Script | PowerShell 函数（`-Enable` / `-Disable`） | 调用反向参数，回到系统默认；还原点 | 在脚本里用注释勾选 | 直接编辑脚本 | `Sophia -Functions` |
| privacy.sexy | YAML（code / revertCode / functions） | revertCode 回到系统默认 | standard / strict | 只能导出脚本 | 生成 .bat 执行 |
| O&O ShutUp10++ | .cfg（每行 ID 加 +/-） | 2026 年起按会话撤销，首次启动时记录原始状态 | 绿 / 黄 / 红 | .cfg | `/quiet /nosrp` |
| Winhance | C# 代码 | 首次启动时存 UserBackup 快照，外加还原点 | 每项有推荐值和默认值 | 配置文件 | （未核实） |
| Dism++ | Data.xml（State / True / False 三段） | False 分支删除值，回到默认 | Level 0–3（含义未核实） | （未核实） | （未核实） |
| BleachBit | CleanerML（XML + XSD） | 无，靠预览 | 危险项带警告 | （未核实） | `--preview` / `--clean` |

**可以直接借鉴的细节**

- **WinUtil**：
  - 原语化的字段，例如 `registry: [{Path, Name, Type, Value, OriginalValue}]`、`service: [{Name, StartupType, OriginalType}]`。
  - 「预设就是一组 ID」。
  - 文档站由 JSON 自动生成。
- **Win11Debloat**：
  - 快照只记录即将改动的项。原本不存在的值，撤销时直接删除。
  - 界面会先预览哪些能撤销、哪些不能。
  - 用 `reg load` 挂载默认用户的配置，让设置对以后新建的账户也生效。
- **Sophia Script**：
  - 同名的双向接口。
  - 组策略类的值会同步到 gpedit。
  - 启动前做预检，例如是否有待重启、Defender 的状态。
- **privacy.sexy**：
  - 用「函数 + 参数」复用代码。
  - 每条脚本都必须附来源。
  - 编译时校验 ID 唯一，并检查每个推荐级别都有内容。
- **O&O ShutUp10++**：
  - 稳定的短 ID、三色推荐等级、按会话撤销、首次启动时存基线。
  - Premium 版会在 Windows 更新后自动纠正被改回的设置。我们不常驻后台，改为「下次打开时提醒」。
- **Winhance**：用 `IsSubjectivePreference` 字段区分「个人偏好」和「建议」。
- **optimizerNXT**：只执行带仓库签名的配置文件，可以防止数据包被投毒。
- **Dism++ 和 winapp2.ini**：现成的清理规则，带说明、警告和分组，已经包含 QQ、微信（旧路径）、百度网盘、WPS 等国内软件。

**杀软误报案例**

| 项目 | 时间 | 检测名或事件 | 怎么解决的 |
|---|---|---|---|
| ExplorerPatcher | 2022、2024 | Defender 报 HackTool、PUP | 把触发检测的组件拆出去；强调发布包由 GitHub Actions 干净构建 |
| WinUtil | 2024 | 报 TrojanDownloader 等 | 让用户更新病毒库、加排除 |
| privacy.sexy | 2024-08 | 脚本里只要出现字符串「privacy.sexy」就会被报 | 讨论过多种方案，结论未核实 |
| Sophia Script | 2024-12 | Wacatac.B!ml | 提交样本，几天后病毒库修正 |
| Winaero Tweaker | 2020、2026-02 | HackTool、Trojan | 向微软上报，病毒库修正 |
| Winhance | 2026-03 | Defender ASR 规则误报 | 给卸载程序签名并改名 |
| Win11Debloat | 2026-06 | Defender、Bitdefender 误报 | 重构触发检测的脚本 |
| 用 hosts 屏蔽遥测 | 2020 年起 | SettingsModifier:Win32/HostsFileHijack | 这种行为本身就被判定为风险 |
| no-defender | 2024-06 | 被 GitHub 按 DMCA 下架 | 原因是打包了 Avast 的二进制文件 |

结论：误报大多只能被动解决。能主动做的只有三件事：拆出高风险组件、做代码签名、改写触发检测的代码。

### 3.4 软件安装、卸载与磁盘分析

**Ninite**（家用免费，靠 Pro 版收费）

- 装机和更新是同一个动作：勾选一次，同一个安装器可以反复运行，已经是最新的就跳过。
- 只从发布者官网下载，并校验数字签名。
- 自动拒绝工具栏。
- 它能被信任，是因为规则简单、公开，而且由付费业务养活免费业务。
- 目录封闭，没有国产软件。

**UniGetUI**（MIT；2026-03 被 Devolutions 收购，仍然免费开源）

- **功能**：
  - 11 个包管理器的图形界面。
  - 可以按软件跳过某个版本，或者忽略小版本更新。
  - 软件列表可以导出、导入。
  - 2026.3.0 起，在详情页显示安装包的域名。
- **2026 年的安全公告**：
  - CVE-2026-10696：按名称子串，把已装的软件错配到攻击者提交的包；用户点更新时，就会执行恶意安装器。
  - CVE-2026-92219：软件包的版本号和 ID 字段能注入命令。
  - CVE-2026-92556：导入设置时存在路径穿越。
- **仿冒**：wingetui.com、unigetui.com 被官方点名为仿冒站。

**winget 在国内**

- **导出与配置**：export / import 的 JSON 只存来源和包 ID，可以带版本号。configure（DSC）对普通用户太重，而且在国内拉取模块很慢。
- **哈希不匹配**：
  - 厂商在同一个 URL 下直接替换安装包，微信就多次出过这个问题。
  - 以管理员身份运行时，无法跳过哈希校验。
- **镜像只加速索引和清单**：
  - 中科大、南大的 winget-source 都是这样（实测过目录）。
  - 改源需要管理员权限；1.8 起还要加 `--trust-level trusted`；可以用 `source reset` 还原。
- **下载快慢，看安装包放在哪**：放在 GitHub 上的很慢；国产软件多放在国内 CDN，反而快。
- **国产软件的 ID 坑**（2026-09 实测索引）：
  - 微信：`Tencent.WeChat` 停在 3.9；4.x 对应的是 `Tencent.WeChat.Universal`。
  - QQ：`Tencent.QQ` 是经典版，新版是 `Tencent.QQ.NT`。
  - WPS：有国际版、`.CN`、`.x64` 三个 ID。
  - 钉钉：`Alibaba.DingTalk.Mainland` 停在 2023 年的 7.1.0，官网已经到 8.x。
  - 火绒、向日葵、酷狗、腾讯电脑管家都没有收录。
- **msstore 源**：
  - 可能遇到证书固定导致的 0x8a15005e 错误；首次使用要接受协议。
  - 有地区限制，例如微信的条目只对中国区开放。
- **没有应用商店的系统**（LTSC）：只能手动安装 winget 本身。

**微软商店**（中国区，实测）

- **收录情况**：
  - 有：微信（和官网同步）、WPS（版本落后）、腾讯会议、百度网盘、搜狗输入法、网易云音乐、QQ 音乐、迅雷、飞书、抖音、剪映、B 站、夸克、向日葵、ToDesk、火绒等。
  - 钉钉搜不到；QQ、飞书的条目没有安装器数据。
- **商店政策可以直接拿来当自律标准**：
  - 10.2.9：下载地址带版本号、提交后安装包不能再改、必须能静默安装、不能是下载器。
  - 10.2.3：不得推装第三方软件。

**Scoop、Chocolatey、Patch My PC**

- **Scoop**：没有官方国内镜像。社区方案是把 GitHub 地址改写到第三方代理，等于把信任交给了第三方。
- **Chocolatey**：社区库每个 IP 每分钟限 20 个包，国内也没有镜像。
- **Patch My PC Home Updater**：可以逐个应用禁用自动更新，或跳过某次更新；目录以欧美软件为主。

**卸载工具**

- **BCUninstaller**（Apache-2.0，v6.3，2026-09）：
  - 残留的置信度由多条可解释的规则累加，默认只勾选高分项。
  - 同一时间只运行一个带界面的卸载程序。
  - 控制台支持 dry-run。
  - 二进制不再做代码签名之后，杀软误报明显增多。
- **Revo Free**：
  - 卸载前建还原点，并完整备份注册表。
  - 残留扫描分三档，只删除确认由该程序创建的注册表项；文件默认进回收站。
- **Geek、HiBit**：HiBit 带注册表清理功能，不建议借鉴。

**磁盘分析**

- **WizTree**：直接读 NTFS 的 MFT，非常快；硬链接不重复计数；个人免费。
- **WinDirStat**（GPLv2）：2024 年重写为 2.x，2.5 起也能读 MFT。
- **SpaceSniffer**：边扫描边绘制色块图，点击可以逐层下钻。
- **TreeSize Free**：树形列表，配占比条。
- **借鉴**：
  - MFT 直读
  - 色块图配可排序的列表
  - 硬链接去重（否则 WinSxS 会显得虚高）

**国内下载渠道**

- **360 软件宝库**：主按钮是「通过 360 软件管家下载」，「普通下载」放在次要位置。
- **腾讯软件中心**：标注「安全认证、无插件」，但页顶大力推广电脑管家。
- **第三方下载站**：
  - 「高速下载器」捆绑在 2022 年被 3·15 曝光。
  - 火绒 2025-11 披露：鲁大师「官网包不含推广插件，其他渠道包内置」。

### 3.5 重装、启动盘、硬件与驱动

**Rufus**（GPLv3；4.15，2026-06）

- **「Windows 用户体验」里的选项**：
  - 去除 TPM、安全启动和 4GB 内存的要求
  - 去除联网和微软账户的要求
  - 按指定的用户名建本地账户
  - 复制本机的区域设置
  - 跳过隐私问题
  - 禁用 BitLocker 自动加密
  - 屏蔽 Copilot、Teams 等
  - 「Windows CA 2023」签名引导（4.10 起，需要 25H2 镜像）
- **借鉴**：它的 FAQ 说，所有调整都用微软文档化的方法，可逆，不增删安装文件。
- **避免**：「不提示，直接清空磁盘安装」这类选项。

**Ventoy**（GPLv3+；1.1.17，2026-07）

- **证书**：1.1.14 起，为 UEFI CA 2023 更换了签名，用户需要重新注册密钥。
- **争议**：
  - 2024 年起，有人要求移除仓库里没有源码的二进制文件。作者补了一份清单，但承诺的 CI 构建没有落地，相关 issue 至今没关。
  - 所有安装共用同一个 MOK 密钥，注册之后，等于信任任何 Ventoy 引导。
- **借鉴**：auto_install 插件可以给一个镜像挂多个应答模板。

**国产 PE**

- **各家情况**：
  - 微 PE：免费闭源，口碑最好。官方最新仍是 2023 年的 v2.3（是否有新版未核实）。
  - Edgeless：部分开源（MPL-2.0）。
  - HotPE：自研代码 MIT 开源。
  - FirPE：称按 MPL-2.0 开源，开源范围未核实。
  - 优启通：PE 里的浏览器默认打开 2345 导航；有第三方称 2026 年起捆绑更严重（未核实）。
- **教训**：闭源的免费版靠推广变现，容易变质；仿冒站和第三方「增强版」泛滥。

**绕过工具**

- **MediaCreationTool.bat**：2023-12 之后停更。
- **UUP dump**：在本地合成 ISO，不是官方渠道，成品也没法对照微软公布的哈希。
- **Flyoobe**（原 Flyby11）：2025-02 被 Defender 标为 PUA，后来的版本已解除；官方提醒 flyoobe.com 是仿冒站。
- **大环境**：
  - 微软 2025-02 删除了注册表绕过方法的文档。
  - 2025-10 起，预览版封堵了 bypassnro 等本地账户技巧；应答文件仍然是受支持的方式。
  - 24H2 起，CPU 必须支持 SSE4.2 和 POPCNT，任何工具都绕不过。

**schneegans unattend-generator**（MIT，.NET 8 库）

- **覆盖范围**：网页版支持 Win10 和 Win11，直到 26H2。
- **能生成的设置**：
  - 语言、区域、时区
  - 跳过 Win11 检查、分区、选择版本
  - 本地账户、开始菜单、任务栏
  - Defender 和 UAC、阻止 BitLocker、删除预装应用、自定义脚本
- **限制**：系统自带的 PowerShell 5.1 加载不了它；GitHub 上也没有 Release，需要自己编译。
- **我们的用法**：只移植 Rufus 那几项，和计划书第五节冲突的选项一律不提供。

**国内一键重装的安全事件**

- **老毛桃、大白菜、晨枫**（火绒 2020-09-01 通报）：
  - 在 PE 阶段，从伪装成 IntelRaid.sys 的文件里释放推广程序。
  - 删除火绒、360 等杀软，装上 360 套装和 2345 浏览器，并篡改书签。
  - 三者同属东莞市互泰网络科技。
- **「装机助理」**（火绒 2021-04）：重装后首次启动时，释放 360、电脑管家、火绒的配置文件，给自己加信任、锁首页，并在桌面塞推广快捷方式。
- **2345「王牌技术员联盟」**：按装机量和锁首页的天数给技术员积分，至今仍在运营。
- **小白一键重装、装机吧、系统之家**：没有检索到安全厂商的点名通报（未核实）。

**图吧工具箱**（免费绿色，无捆绑，不写注册表；2026.08）

- **内容**：收录十几类、上百个工具（CPU-Z、GPU-Z、HWiNFO、CrystalDiskInfo、屏幕检测、烤机等），也集成了 Rufus、Ventoy、DDU。
- **主动下架**：2026.08 版因为 ThrottleStop 的提权驱动有漏洞，暂时把它下架了。
- **被信任的原因**：社区出身，多年保持纯净，按用途分类，主动下架有风险的组件。
- **风险**：仿冒站泛滥；官网被必应国内版屏蔽（见官方公告）。

**硬件信息**

- **HWiNFO**：用自己的驱动，官方称从未用过 WinRing0。
- **CrystalDiskInfo**（MIT，9.9.2）：
  - 判断口径：机械硬盘看 05、C5、C6 这几项的原始值；固态硬盘剩余寿命 ≤10% 时显示「注意」。
  - 官方强调「先备份再排查」，也承认可能误判。
  - 官网「快速下载」给的是带广告的 *Ads.exe；winget 清单也曾收录过这个版本。
- **鲁大师**：2019 年的招股书显示 360 间接持股 41.37%，跑分的公信力存疑。

**驱动**

- **SDIO**（GPLv3）：原版 SDI 在 2016–17 年夹带推广，前开发者因此分叉出了 SDIO。
- **DDU**（MIT）：推荐在安全模式下「清理并重启」。它建议让杀软排除自己的目录，这一点我们不照搬。
- **驱动总裁「万能网卡版」**：能在 PE 里预先注入网卡、USB、磁盘控制器的驱动，是懂哥装机的主力。有用户反馈它要微信扫码、默认勾选捆绑（未核实）。

**验机**

- **热度**：B 站「图吧工具箱验机流程 + 烤机」有 467.9 万播放。
- **常见做法**：
  - 看硬盘的通电时间和通电次数。新机一般在几十小时、几十次以内，各家口径不一。
  - 用 `powercfg /batteryreport` 看电池循环次数（新机 0–3 次），以及满电容量 ÷ 设计容量（≥95%）。
  - 核对序列号和保修日期，测试屏幕坏点和键盘。
- **系统自带**：Win11 的设置已经能显示 NVMe 硬盘的剩余寿命、备用空间和温度；SATA 盘不显示。
- **教训**：阈值口径不统一，不能一刀切，只能给参考范围。

### 3.6 AI 与电脑维护（2025–2026）

- **微软**：
  - **设置里的 AI 代理**：用本地小模型 Mu，只在 Copilot+ PC 上可用。它支持简体中文，但地区限制写明「中国除外」。它只给建议，用户明确要求才改，改了可以撤销。
  - **Copilot Actions**：在独立账户的沙盒里运行，默认关闭。微软自己承认，跨提示注入可能导致数据外泄。
  - **Windows 上的 MCP**：仍在预览，内置文件资源管理器和设置两个连接器。
  - **收缩**：2026-03 起，微软收缩了 Copilot 的入口，转而抓系统质量。
- **国内**：
  - 360 纳米 AI、腾讯元宝入口、荣耀 YOYO、华为小艺主要做搜索、写作和 PPT。
  - 2026 年，「龙虾」（OpenClaw 等本地 AI 智能体）的管理和防护成了标配，腾讯、360、微软电脑管家、联想都已上线。
  - 真正面向修电脑的，只有毒霸 AI 助手、联想的管家智能体和「小天」在尝试。
  - 豆包电脑版的「操作电脑」要用户手动授权，操作日志留在本地。
- **结论**：
  - AI 只负责「听懂人话，匹配到症状卡片」。
  - 它能执行的只能是数据文件里已有的、可撤销的动作，而且每一步都要用户确认。
  - 绝不让模型生成命令后直接执行。

---

## 四、取长补短清单

| # | 借鉴点 | 来源 | 计划书位置 | 版本 |
|---|---|---|---|---|
| 1 | 症状卡片（标题 / 症状 / 原因 / 修复），加上成熟度（图文 → 半自动 → 一键） | 腾讯电脑诊所 | 6.2 | v0.1 |
| 2 | 用日常说法搜索症状 | 腾讯电脑诊所 | 4.1 症状搜索 | v0.1 |
| 3 | 断网检测清单（7 项 + 8 项）、逐项亮灯、修复分轻重 | 360、火绒 | 4.2 上不了网 | v0.1 |
| 4 | 网络修复离线可用、能单独带走 | 360 断网急救箱离线版 | 4.2 上不了网 | v0.1 |
| 5 | 操作记录 + 恢复原状 | 火绒 | 4.1 修改日志 | v0.1 |
| 6 | 运行时快照撤销、按会话撤销、首次启动存基线 | Win11Debloat、O&O、Winhance | 6.4 | v0.1 |
| 7 | 检测是一等公民；大版本更新后提醒被改回的设置 | Dism++、WinUtil、O&O | 6.2 | v0.1 |
| 8 | 三级推荐等级，和可撤销性挂钩 | O&O、privacy.sexy、火绒 | 6.2、4.4 | v0.2 |
| 9 | 预设就是一组 ID；导入前先预览 | WinUtil、Win11Debloat | 4.5 预设套餐 | v0.2 |
| 10 | 设置也能应用到以后新建的用户 | Win11Debloat | 4.4 常用设置 | v0.2 |
| 11 | 组策略类的设置同步到 gpedit | Sophia Script | 4.4 常用设置 | v0.2 |
| 12 | 现成的清理规则 | Dism++（MIT）、winapp2（CC BY-SA 4.0） | 6.2、4.2 C 盘 | v0.1 |
| 13 | 修复前强制备份，备份失败就中止 | Tweaking.com、Complete Internet Repair、NetAdapter Repair | 6.4 | v0.1 |
| 14 | 缓存目录改名而不删除；服务没停下来就中止 | wureset | 4.2 更新失败 | v0.2 |
| 15 | 先验证签名再隐藏微软项；禁用而不删除 | Autoruns | 4.2 开机慢 | v0.1 |
| 16 | 删除的项目进隔离区，可以恢复 | AdwCleaner、Revo | 4.4 卸载残留 | v0.2 |
| 17 | 蓝屏只给嫌疑名单和把握大小 | BlueScreenView、WhoCrashed | 4.2 蓝屏 | v0.2 |
| 18 | 每一项都说明「这是什么、能不能修、怎么修」 | WhyNotWin11、FixWin | 4.4 Win11 检测 | v0.0 |
| 19 | 勾选一次、反复运行；校验安装包签名 | Ninite | 4.4 装机必备 | v0.2 |
| 20 | 可解释的残留评分；同一时间只跑一个卸载程序 | BCUninstaller | 4.4 卸载 | v0.2 |
| 21 | MFT 直读、色块图、硬链接去重 | WizTree、WinDirStat | 4.2 C 盘 | v0.1 |
| 22 | 应答文件的选项 | Rufus | 4.6 启动盘 | 1.x |
| 23 | 能解释、能调整的硬盘健康阈值 | CrystalDiskInfo | 4.4 硬盘健康、4.5 验机 | v0.0 |
| 24 | 按部件单项检测；只共享部分窗口的隐私设计 | 华为电脑管家 | 4.2、4.5 报告 | v0.2 |
| 25 | 修复时保留用户自己的设置 | 微软电脑管家的任务栏修复 | 原则 10 | 全程 |
| 26 | 只执行签过名的数据包 | optimizerNXT | 5.1 第 8 条 | 以后 |
| 27 | AI 的动作走白名单、先确认、可撤销 | Win11 设置里的 AI 代理 | 4.8 | 1.x |

---

## 五、反面教训清单

| 教训 | 案例 | 我们的对策 |
|---|---|---|
| 捆绑、静默安装、弹窗关不掉 | 360、金山毒霸、鲁大师（见多份火绒报告） | 第五节第 2 条；原则 4 |
| 付费修复修不好 | 用户投诉金山、鲁大师的「DLL 修复」 | DLL 缺失用官方运行库免费修（4.2） |
| 体检打分、制造焦虑 | 360「开机击败全国 x%」 | 原则 7 |
| 把用户自己的选择当成问题 | 微软电脑管家劝用户改回 Bing | 原则 7 |
| 上传用户数据做分析 | 360 蓝屏修复器上传 dump 文件 | 只在本地分析（4.2 蓝屏） |
| 撤销时写回写死的默认值 | WinUtil、privacy.sexy、Sophia Script | 运行时快照（6.4） |
| 批量重置权限 | Tweaking.com Windows Repair | 第五节第 28 条 |
| 清掉单位下发的策略 | wureset 删除 `Policies\WindowsUpdate` | 4.2 更新失败、4.3 |
| 本地服务、URL 协议，加上校验不严 | Dell、ASUS、联想、MSI；Follina | 6.5 不开本地服务 |
| 按名称子串匹配、拼接命令、导入不校验 | UniGetUI 2026 年的三个漏洞 | 6.5 导入文件；4.4 精确 ID |
| 使用第三方内核驱动 | WinRing0、ThrottleStop；毒霸用内核驱动绕过自我保护 | 6.5 不装内核驱动 |
| 装系统时植入推广、删杀软、锁主页 | 老毛桃、大白菜、晨枫；装机助理；2345 技术员联盟 | 第五节第 29、30 条 |
| 卸载时留后门 | 驱动精灵（火绒 2019 年报告） | 卸载干净，只可选保留修改日志 |
| 升级通道被劫持 | 驱动人生（2018） | 6.5 供应链；不静默更新 |
| 仿冒官网和下载站 | 火绒、UniGetUI、图吧、Flyoobe 都遇到过 | 唯一官方下载地址、商标、签名（6.9、第九节） |
| 单人维护、项目停更 | privacy.sexy、Dism++、optimizer、MediaCreationTool.bat | 数据和引擎分开，尽早找第二位维护者 |
| 失去代码签名，误报增多 | BCUninstaller | 6.9 签名 |

---

## 六、我们的差异化

- **别人没做到的**：
  - 按症状修复、每步可撤销（运行时快照）、开源、中文、零推广，五样同时具备。
  - 专门修国内的高频问题：打印机与共享、梯子残留、DLL 缺失、微信占满 C 盘。
  - 把被别的工具改坏的系统修回来。
  - 给懂哥的「报告 + 药方」，替代远程协助。
  - 不依赖云的重装前备份。
- **故意不做的**：常驻拦截、杀毒、驱动库、PE、激活、一键加速。需要这些的用户，推荐可信的工具（火绒、图吧工具箱、微 PE、Rufus），我们不去替代。
- **和系统自带能力的关系**：把「获取帮助」、重装当前版本、时间点还原、重置此电脑编进修复阶梯，帮用户在合适的时候用上它们。
- **可以和火绒、微软电脑管家共存**：它们管常驻防护、清理和加速，我们管诊断、修复和重装前后。

---

## 七、待进一步核实

- 国内能不能登记 Win10 消费者 ESU。
- 微软商店对需要管理员权限的 Win32 程序有什么要求。
- winapp2.ini 是否覆盖微信 4.x 的新路径；Dism++ 规则里 Level 0–3 的含义。
- 微 PE 在 2023 年之后是否有新版本；优启通、驱动总裁的捆绑情况。
- 腾讯软件中心的安装包是否附带推荐；小白一键重装等工具是否被安全厂商通报过。
- Winhance、Dism++ 的命令行和导出能力。
- 360、火绒、腾讯电脑管家的白名单申报流程和周期。

---

## 参考来源

**3.1 国内综合管家与急救工具**

- 360：[断网急救箱](https://weishi.360.cn/work/dwjjx/)、[离线版说明](https://baoku.360.cn/sinfo/1900055951_9510099.html)、[系统急救箱](https://baoku.360.cn/aqsd/221.html)、[人工服务下线公告](https://bbs.360.cn/thread-16127052-1-1.html)、[弹窗报道](https://news.qq.com/rain/a/20250315A07XH900)、[Win10 盾甲](https://news.zol.com.cn/1063/10630242.html)
- 腾讯：[电脑诊所帮助](https://guanjia.qq.com/help/dnzs.html)、[问题列表](https://guanjia.qq.com/web_clinic/list/2_list.html?title=)、[一键装机](https://guanjia.qq.com/help/yjzj.html)、[电脑管家 18.0](https://www.leiphone.com/category/industrynews/U8ZlncybH3z5dyXJ.html)
- 金山：[电脑医生](https://www.ijinshan.com/functions/pcdoctor.html)、[火绒 2026 年报告](https://www.huorong.cn/document/tech/vir_report/1985)、[火绒 2025 年报告](https://www.huorong.cn/document/tech/vir_report/1834)、[黑猫投诉](https://tousu.sina.com.cn/complaint/view/17375043277/)
- 火绒：[弹窗拦截](https://www.huorong.cn/document/info/classroom/2035)、[启动项管理](https://www.huorong.cn/document/info/productions/119)、[仿冒预警](https://www.huorong.cn/document/info/classroom/1888)
- 微软电脑管家：[官网](https://www.microsoft.com/zh-cn/windows/pc-manager)、[劝用户改回 Bing（Windows Latest）](https://www.windowslatest.com/2024/05/16/microsoft-pc-manager-wants-you-to-repair-windows-11-by-turning-on-bing-search/)、[龙虾管理](https://pcmhomepage.chinacloudsites.cn/zh-cn/news/detail/n21)、[新浪评测](https://finance.sina.com.cn/tech/roll/2025-07-11/doc-infezuvz9707047.shtml)
- 软媒魔方：[官网](https://mofang.ruanmei.com/)
- 厂商管家：[联想电脑管家更新日志](https://guanjia.lenovo.com.cn/history.html)、[华为电脑管家](https://consumer.huawei.com/cn/support/pc-manager/)、[华为远程服务](https://consumer.huawei.com/cn/support/content/zh-cn16007354/)、[荣耀 YOYO](https://www.honor.com/cn/tech/pc-yoyo-assistant-2/)
- 反面参照：[火绒：鲁大师云控推广（2025-11）](https://www.huorong.cn/document/tech/vir_report/1858)、[火绒：驱动精灵（2019）](https://www.huorong.cn/document/tech/vir_report/802)、[鲁大师投诉](https://tousu.sina.com.cn/complaint/view/17375321681)、[驱动人生升级通道被劫持](https://www.secrss.com/articles/7146)

**3.2 国际修复工具与微软自带能力**

- 修复工具：[FixWin 11](https://www.thewindowsclub.com/fixwin-windows-pc-repair-software)、[Tweaking.com Windows Repair](https://www.tweaking.com/features/windows-repair-all-in-one/)、[NetAdapter Repair](https://github.com/Archer1613/netadapter-repair)、[Complete Internet Repair](https://rizonesoft.com/downloads/complete-internet-repair/)、[script-wureset](https://github.com/wureset-tools/script-wureset)、[微软：重置 Windows 更新](https://learn.microsoft.com/en-us/troubleshoot/windows-client/installing-updates-features-roles/additional-resources-for-windows-update)
- 获取帮助与已移除的组件：[Windows 疑难解答](https://support.microsoft.com/en-us/support/get-help/windows-troubleshooters)、[MSDT 弃用](https://support.microsoft.com/en-us/windows/deprecation-of-microsoft-support-diagnostic-tool-msdt-and-msdt-troubleshooters-0c5ac9a2-1600-4539-b9d0-069e71f9040a)、[WMIC 移除](https://support.microsoft.com/en-us/servicing/os/windows/docs/2025/09/windows-management-instrumentation-command-line-wmic-removal-from-windows)、[SaRA 移除](https://support.microsoft.com/en-us/servicing/os/windows/docs/2026/03/microsoft-support-and-recovery-assistant-sara-command-line-utility-removal-from-windows)、[Follina 指引](https://www.microsoft.com/en-us/msrc/blog/2022/05/guidance-for-cve-2022-30190-microsoft-support-diagnostic-tool-vulnerability)
- 恢复能力：[快速机器恢复](https://learn.microsoft.com/en-us/windows/configuration/quick-machine-recovery)、[恢复选项](https://support.microsoft.com/en-us/windows/recovery-options-in-windows-31ce2444-7de3-818c-d626-e3b5a3024da5)、[重装当前版本](https://support.microsoft.com/en-us/windows/deployment/install-upgrade/fix-issues-by-reinstalling-the-current-version-of-windows)、[时间点还原正式推送](https://campustechnology.com/articles/2026/07/06/point-in-time-restore-now-generally-available-for-windows-11.aspx)
- 备份与迁移：[Windows 备份](https://support.microsoft.com/en-us/windows/experience/backup-recovery/back-up-and-restore-with-windows-backup)、[PC 到 PC 迁移被撤下（Windows Latest）](https://www.windowslatest.com/2026/09/16/microsoft-built-a-local-pc-to-pc-transfer-tool-for-windows-11-now-its-killing-it-to-push-onedrive-based-solution/)、[Laplink × Intel](https://news.laplink.com/press-releases/intel-selects-laplink-as-pc-migration-partner-to-power-windows-11-transition)
- 检测工具：[PC 健康检查](https://support.microsoft.com/en-us/windows/how-to-use-the-pc-health-check-app-9c8abd9b-03ba-4e67-81ef-36f37caa7844)、[WhyNotWin11](https://github.com/rcmaehl/WhyNotWin11)、[Autoruns](https://learn.microsoft.com/en-us/sysinternals/downloads/autoruns)、[AdwCleaner 文档](https://toolslib.net/downloads/viewdownload/1-adwcleaner/pages/5-en-adwcleaner-documentation/)、[BlueScreenView](https://www.nirsoft.net/utils/blue_screen_view.html)、[WhoCrashed](https://www.resplendence.com/whocrashed)
- 厂商管家漏洞：[Dell SupportAssist RCE](https://billdemirkapi.me/remote-code-execution-on-most-dell-computers/)、[ASUS DriverHub](https://mrbruh.com/asusdriverhub/)、[Lenovo Vantage](https://atredis.com/blog/2025/7/7/uncovering-privilege-escalation-bugs-in-lenovo-vantage/)、[SensePost：MSI Center 等四款工具](https://sensepost.com/blog/2025/pwning-asus-driverhub-msi-center-acer-control-centre-and-razer-synapse-4/)

**3.3 开源设置 / 优化 / 清理工具**

- WinUtil：[仓库](https://github.com/ChrisTitusTech/winutil)、[tweaks.json](https://raw.githubusercontent.com/ChrisTitusTech/winutil/main/config/tweaks.json)、[自动化说明](https://winutil.christitus.com/guides/automation/)
- Win11Debloat：[仓库](https://github.com/Raphire/Win11Debloat)、[PR #566（运行时快照）](https://github.com/Raphire/Win11Debloat/pull/566)、[撤销说明](https://github.com/Raphire/Win11Debloat/wiki/Reverting-Changes)
- Sophia Script 与 privacy.sexy：[Sophia Script](https://github.com/farag2/Sophia-Script-for-Windows)、[privacy.sexy](https://github.com/undergroundwires/privacy.sexy)、[privacy.sexy 数据格式](https://raw.githubusercontent.com/undergroundwires/privacy.sexy/master/docs/collection-files.md)
- 其他优化工具：[O&O ShutUp10++ 更新日志](https://www.oo-software.com/en/shutup10/changelog)、[Winaero Tweaker](https://winaero.com/winaero-tweaker/)、[optimizer](https://github.com/hellzerg/optimizer)、[optimizerNXT](https://github.com/hellzerg/optimizerNXT)、[Winhance](https://github.com/memstechtips/Winhance)
- 清理规则与许可证：[BleachBit CleanerML](https://docs.bleachbit.org/cml/cleanerml.html)、[winapp2.ini](https://github.com/MoscaDotTo/Winapp2)、[CC BY-SA 4.0 单向兼容 GPLv3](https://creativecommons.org/2015/10/08/cc-by-sa-4-0-now-one-way-compatible-with-gplv3/)
- Dism++：[规则仓库](https://github.com/Chuyu-Team/Dism-Multi-language)、[Data.xml](https://raw.githubusercontent.com/Chuyu-Team/Dism-Multi-language/master/Data.xml)、[#719（为什么不开源）](https://github.com/Chuyu-Team/Dism-Multi-language/issues/719)
- 误报与下架：[ExplorerPatcher 误报说明](https://github.com/valinet/ExplorerPatcher/wiki/Antivirus-false-positives)、[privacy.sexy #304](https://github.com/undergroundwires/privacy.sexy/issues/304)、[HostsFileHijack](https://www.bleepingcomputer.com/news/microsoft/windows-10-hosts-file-blocking-telemetry-is-now-flagged-as-a-risk/)、[GitHub DMCA：no-defender](https://github.com/github/dmca/blob/master/2024/06/2024-06-07-gen-digital.md)

**3.4 软件安装、卸载与磁盘分析**

- Ninite 与 UniGetUI：[Ninite 工作原理](https://ninite.com/help/how-ninite-works/)、[UniGetUI](https://github.com/Devolutions/UniGetUI)、[DEVO-2026-0019](https://devolutions.net/security/advisories/DEVO-2026-0019/)、[DEVO-2026-0033](https://devolutions.net/security/advisories/DEVO-2026-0033/)
- winget：[export](https://learn.microsoft.com/en-us/windows/package-manager/winget/export)、[import](https://learn.microsoft.com/en-us/windows/package-manager/winget/import)、[configure](https://learn.microsoft.com/en-us/windows/package-manager/winget/configure)、[中科大镜像](https://mirrors.ustc.edu.cn/help/winget-source.html)、[南大镜像](https://mirrors.nju.edu.cn/winget-source/)、[钉钉清单](https://github.com/microsoft/winget-pkgs/tree/master/manifests/a/Alibaba/DingTalk/Mainland)
- 微软商店：[商店政策](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)、[个人开发者免费注册](https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/)
- 其他包管理器：[Scoop](https://github.com/ScoopInstaller/Scoop)、[Chocolatey 审核流程](https://docs.chocolatey.org/en-us/community-repository/moderation/)、[Patch My PC Home Updater](https://patchmypc.com/home-updater)
- 卸载工具：[BCUninstaller](https://github.com/BCUninstaller/Bulk-Crap-Uninstaller)、[残留置信度规则](https://github.com/BCUninstaller/Bulk-Crap-Uninstaller/blob/master/source/UninstallTools/Junk/Confidence/ConfidenceRecords.cs)、[Revo Free](https://www.revouninstaller.com/products/revo-uninstaller-free/)
- 磁盘分析：[WizTree](https://diskanalyzer.com/)、[WinDirStat](https://windirstat.net/)、[SpaceSniffer](https://www.uderzo.it/main_products/space_sniffer/)、[TreeSize Free](https://www.jam-software.com/treesize_free)
- 国内下载渠道：[360 软件宝库](https://baoku.360.cn/sinfo/102112879_4002732.html)、[火绒：360 极速版](https://www.huorong.cn/document/tech/vir_report/861.html)、[「高速下载器」曝光](https://www.163.com/dy/article/H2J2BDQ10534P59R.html)

**3.5 重装、启动盘、硬件与驱动**

- 启动盘：[Rufus ChangeLog](https://raw.githubusercontent.com/pbatard/rufus/master/ChangeLog.txt)、[Rufus FAQ](https://github.com/pbatard/rufus/wiki/FAQ)、[Ventoy 更新](https://www.ventoy.net/en/doc_news.html)、[Ventoy #2795](https://github.com/ventoy/Ventoy/issues/2795)、[Ventoy #2184](https://github.com/ventoy/Ventoy/issues/2184)、[auto_install 插件](https://www.ventoy.net/en/plugin_autoinstall.html)
- PE：[Edgeless](https://github.com/EdgelessPE/Edgeless)、[HotPE](https://github.com/VirtualHotBar/HotPEToolBox)、[FirPE](https://www.firpe.cn/)
- 绕过工具与系统要求：[UUP dump](https://uupdump.net/)、[Flyoobe](https://github.com/builtbybel/FlyOOBE)、[24H2 的 SSE4.2 要求](https://www.tomshardware.com/software/windows/microsoft-updates-windows-11-24h2-requirements-cpu-must-support-sse42-or-the-os-will-not-boot)、[账户绕过被封堵（Windows Latest）](https://www.windowslatest.com/2026/09/13/microsoft-blocked-every-windows-11-account-bypass-except-the-most-boring-link-on-its-own-page/)
- 应答文件：[unattend-generator 仓库](https://github.com/cschneegans/unattend-generator/)、[网页版](https://schneegans.de/windows/unattend-generator/)
- 国内重装事件：[火绒：老毛桃事件](https://www.huorong.cn/document/tech/vir_report/834)、[装机助理](https://zhuanlan.zhihu.com/p/364877761)、[2345 技术员联盟](https://jifen.2345.com/)
- 图吧工具箱：[官网](https://www.tbtool.cn/)、[公告](https://www.tbtool.cn/gonggao/)
- 硬件信息：[HWiNFO 许可](https://www.hwinfo.com/licenses/)、[CrystalDiskInfo 健康判定](https://crystalmark.info/en/software/crystaldiskinfo/crystaldiskinfo-health-status/)、[winget 收录广告版的讨论](https://github.com/microsoft/winget-pkgs/issues/288639)
- 驱动：[SDIO](https://www.glenn.delahoy.com/snappy-driver-installer-origin/)、[DDU](https://github.com/Wagnard/display-drivers-uninstaller)
- 系统能力：[Win11 只显示 NVMe 的硬盘健康](https://windowsforum.com/news/windows-11-drive-health-nvme-only-not-sata-ssds-or-hdds.437674/)、[用 PowerShell 检查 CA 2023 证书](https://mikefrobbins.com/2026/02/12/verify-windows-uefi-ca-2023-certificate-with-powershell/)

**3.6 AI 与电脑维护**

- 微软：[Mu 与设置里的 AI 代理](https://blogs.windows.com/windowsexperience/2025/06/23/introducing-mu-language-model-and-how-it-enabled-the-agent-in-windows-settings/)、[设置 AI 代理的配置](https://learn.microsoft.com/en-us/windows/configuration/settings/agent)、[实验性智能体功能](https://support.microsoft.com/en-us/windows/experimental-agentic-features-a25ede8a-e4c2-4841-85a8-44839191dfb3)、[Windows 上的 MCP](https://learn.microsoft.com/en-us/windows/ai/mcp/overview)、[微软收缩 Copilot](https://thejournal.com/articles/2026/03/30/microsoft-scales-back-copilot-integrations-in-windows-11.aspx)
- 国内：[360 安全龙虾](https://www.bbtnews.com.cn/2026/0315/587182.shtml)
