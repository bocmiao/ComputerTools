# 功能缺口与可复用资源调研（2026 年 9 月 28 日）

> 配合[《竞品调研与取长补短》](competitive-analysis.md)使用：那一份讲产品和公司（信任问题、撤销机制、安全教训），这一份**逐项对照功能**，找**能直接拿来用的数据和代码**，最后给出按优先级排好的开发计划。
>
> - **方法**：分四路调研，各自核对 2025–2026 年的现状：①国内竞品的逐项功能清单（腾讯、360、金山、火绒、微软电脑管家、联想、华为、荣耀、小米）；②国外和开源修复工具的完整修复项（FixWin、Tweaking Windows Repair、微软「获取帮助」、PowerToys、NirSoft、Sysinternals）；③GitHub 上可复用的数据和代码，逐个核对许可证；④国内用户的高频问题和微软官方错误码资料。
> - **口径**：每条事实附来源链接。只有单一来源、二手来源或社区来源的标「（未核实）」或「（社区）」；修法以微软官方文档为准。浏览量这类数字是 2026-09-28 从官方接口逐页统计的；Star 数、许可证来自 GitHub API；数据文件从 raw.githubusercontent.com 下载原文逐条核对，文中「L 行号」是当天默认分支上的行号。
> - **许可证**：小药箱是 GPL-3.0。MIT、Apache-2.0、BSD、LGPL-3.0、GPL-3.0-or-later 能直接并进来；CC BY-SA 4.0 单向兼容 GPLv3（署名、写明修改和日期）；AGPL-3.0 只取事实（注册表路径和数值是事实，说明文字和编排受保护）；GPL-2.0-only、没有许可证、闭源的只借思路。
> - **约束**：所有建议都按[计划书](plan.md)第五节「我们不做什么」过了一遍：策略、UAC、防火墙、Defender、IFEO、更新策略只检测不改；不加常驻；不装驱动；新依赖要先征得同意。

---

## 一、结论先行

1. **拿到了两份硬的国内需求数据。** 联想知识库 114 个一键小工具带浏览量（「Quick Fix」），腾讯电脑管家知识库每篇文章带浏览量。最热的单项是**「关闭 Windows 自动更新」：Win10、Win11 两个工具合计 600 万浏览**；其后是驱动安装 293 万、游戏闪退检测 78 万、C 盘清理 70 万、补丁卸载 24 万、局域网共享 20 万。我们不做「关更新」（计划书第五节第 5 条），但要把系统自带的「暂停更新」「使用时段」「重启前提醒」放到用户面前，并讲清楚为什么不建议关。
2. **覆盖情况。** 腾讯知识库浏览量前 19 名里，我们已经覆盖 10 个、部分覆盖 2 个。整理出的 29 个 A 档高频问题里，已有 5 个、部分有 9 个、**完全没有的 15 个**。
3. **最大的缺口**（按频率 × 能修程度）：WiFi 选项不见了、飞行模式关不掉；显示「无 Internet」其实能上网；局域网共享和 0x80070035（Win11 24H2 起默认要求 SMB 签名、专业版默认拒绝来宾登录，是这两年暴增的主因）；屏幕突然变黑白；软件和解压文件名乱码（UTF-8 Beta）；开机黑屏只有鼠标；快捷方式、exe 打不开；鼠标卡顿、USB 设备断开；WiFi 隔一会儿就断；游戏和软件闪退。这些大部分**能一键修、能撤销**，第八节列为 P0 和 P1。
4. **能直接复用的数据，许可证清楚**：winutil、Win11Debloat、Sophia Script、Win10-Initial-Setup-Script（都是 MIT）里的注册表事实；CrystalDiskInfo（MIT）的硬盘判定规则和中文属性名；CleanMyWechat（MIT）的微信 3.x、4.x、企业微信目录结构；BCUninstaller（Apache-2.0）的卸载残留打分规则；Winapp2（CC BY-SA 4.0）和 BleachBit（GPL-3.0+）的部分清理规则。DriverStoreExplorer 是 GPL-2.0-only，只能借思路。
5. **现成规则里有坑，移植必须逐条审。** Winapp2 的 `[Tencent QQ *]` 会**递归删掉整个「文档\Tencent Files」（聊天记录）**；winutil 的「更新重置」会删整棵 Policies 和组策略目录；Sophia 的临时文件任务在某种情况下删整个 `C:\Recovery`；Dism++ 的「腾讯相关软件下载目录」其实删的是整个 `ProgramData\temp`。
6. **官方资料要补进数据**：Windows 更新错误码 40 个、蓝屏终止代码 30 个（按参数细分）、设备管理器错误码全表（1–57）、打印机和共享的错误码、DLL → 官方运行库对照、NCSI 的默认值。另外：**从 Win11 26H1 起 .NET 3.5 不再是 Windows 功能**，只能用独立安装包；VC++ 的官方固定链接已经换成 `aka.ms/vc14/…`。
7. **微软自带的能力要编进修复阶梯**：「获取帮助」的 10 个自动疑难解答能用 `ms-contact-support://smc-to-emerald/<名字>` 直接调起（MSDT 已经退役）；「用 Windows 更新重装当前版本」保留应用和文件，是最后的兜底；时间点还原会把个人文件一起回滚，要提醒。
8. **「每步可撤销」仍是我们独有的。** 竞品里只有火绒的启动项「恢复原始状态」、腾讯软件搬家的「还原」这类零星做法；360 急救箱只记日志，不能撤销。
9. **量不等于有用。** 腾讯知识库 41,120 篇文章里 98.6% 是 2025 年以后批量生成的，66% 浏览不到 10 次；真正有流量的是 588 篇老文章。我们要在「查得准、修得好、能撤销」上胜出，而不是堆条目。
10. **2025–2026 的新方向**：本地 AI 智能体（「龙虾」）的检查和停止（微软、联想、火绒都上了）；联想的「异常报错弹窗处理」。先观察，列在 P2。
11. **顺手核对了现有功能**：显卡的 live dump 不会算成蓝屏；Win10 延长支持写的是 2027-10-12；认得微信 4.x 的 `xwechat_files`；认得 KB5042320；脚本没有用已移除的 WMIC；24 小时内的还原点被系统跳过时如实告诉用户。这几处都已经做对。
12. **下一步**：先做第八节的 P0（13 项），打头的是 WiFi 不见了、「无 Internet」、局域网共享、屏幕变黑白、乱码、黑屏只有鼠标、快捷方式和 exe 打不开。

## 二、我们现在有什么

截至 2026-09-28（main 分支）：**按症状修 23 个**，**检测 52 项**（体检用其中 27 项），**修复 47 项**（都能撤销），**小工具 56 个**，加上工具箱里的近 20 个本地小工具、开机启动项、右键菜单、「新建」菜单、资源管理器多余图标四个管理页，以及修改日志。

- 按症状修：上不了网、C 盘满了、开机慢、电脑卡、蓝屏、更新失败、没声音、麦克风摄像头、键盘没反应、输入法不见了、蓝牙、U 盘插上没反应、打印机（脱机、连不上、0x0000011b、0x00000709）、搜不到文件、桌面图标不见了、浏览器主页被改、缺 dll、自己开机或唤醒、笔记本电池、老弹广告。
- 下面各节的对照表里：✅ 已有；◐ 部分（有检测没修复，或者只在别的症状里顺带）；✗ 没有。

---

## 三、国内竞品逐项对照

说明：「一键」是点一下软件自动改系统；「图文」只有教程；「工具」是用户在工具里自己勾选；「常驻」要在后台常驻才生效。浏览量这类数字都是 2026-09-28 从官方接口逐页统计的。

### 3.1 腾讯电脑管家：「电脑诊所」已经变成 4 万篇的知识库

**三代形态**：

| 形态 | 内容 | 一键还是图文 |
|---|---|---|
| 旧「修复方案库」（约 2013–2022，[存档](https://web.archive.org/web/20220518073243/https://guanjia.qq.com/web_clinic/list/2_list.html)） | 分类：腾讯专区、桌面图标、上网异常、软件硬件、XP 专区、系统综合、硬件问题；去重后 90 多条，每条写「主要症状 / 可能原因 / 修复步骤」 | **全部一键**：详情页的按钮调起本机电脑管家执行修复步骤 |
| 「问题库」文章（2016–2023） | 电脑故障、电脑安全、网络故障、软件硬件、游戏专区、系统问题…… | 图文 |
| 现行「管家知识库」（2023 起，[首页](https://guanjia.qq.com/knowledge-base/)） | 13 个一级分类、78 个二级分类，共 41,120 篇；电脑软件问题下有「DLL 修复」384 篇、「DirectX 修复」513 篇、「运行库修复」274 篇 | 图文；只有 3.6% 的文章提到「一键修复」，多半是引导打开电脑管家的某个工具 |

所有 `web_clinic` 链接现在都跳到知识库（[一级分类接口](https://guanjia.qq.com/info-platform/api/category/v1/categorys?platformId=1)）。**41,120 篇里 98.6% 是 2025 年以后批量生成的**，大量是「AI xx？」式标题，66% 的文章浏览不到 10 次；真正有流量的是 2025 年以前的 588 篇老文章，占总浏览量的 58%。客户端里，网络修复、dll 文件丢失、浏览器无法上网并进了「电脑诊所」模块；急救箱、漏洞修复、文件粉碎放进「安全工具」；开机启动项、弹窗拦截、右键菜单放进「权限管理」（[知识库 1551](https://guanjia.qq.com/knowledge-base/content/1551)）。

**旧一键修复库的代表条目**：上不了网（修 DNS、HOSTS、IE 代理）；能上 QQ 但打不开网页（清缓存、重新注册组件、修 DNS 和代理、修 Winsock）；开机慢；没有声音（恢复声音服务、启动多媒体驱动）；摄像头打不开；输入法无法使用；显卡驱动问题致显示异常；添加删除程序打不开；音量、网络图标消失；任务栏不显示时间；上网主页异常；火车票网站证书错误；丢失 VC++、d3dx**.dll、MSVBVM60、**eay32；Word 等 Office 文档打不开。约一半是 QQ、Flash、IE 相关，已经过时。桌面图标这一类做得最细：「我的电脑」「回收站」图标没了、快捷方式变未知图标、图标有黑方块、磁盘显示未知图标、删不掉的图标、去掉小箭头和「快捷方式」字样，逐条一键。

**知识库浏览量前 20（真实需求）**：

| 标题 | 浏览量 | 我们 |
|---|---|---|
| 磁盘空间不足怎么清理 | 25.7 万 | ✅ C 盘满了 |
| QQ 能上网，网页打不开 | 17.4 万 | ✅ 上不了网 |
| 使用 VPN 后无法上网 | 11.2 万 | ✅ 失效的代理 |
| Win10 关闭锁屏广告 | 11.0 万 | ✅ 常用设置 |
| Ping 显示「传输失败。常见故障。」 | 8.1 万 | ✗ |
| 手动设 IP 时「已计划将多个默认网关用于提供单一网络」 | 7.9 万 | ◐（查得出手动设置的地址） |
| 重定向次数过多 | 7.8 万 | ✗（多是浏览器 Cookie、时间） |
| 开机启动项禁用 | 7.6 万 | ✅ |
| 中文输入法切换不出来 | 7.1 万 | ✅ 输入法不见了 |
| U 盘无法识别 | 6.4 万 | ✅ |
| Alt+Z 快捷键无法使用 | 5.7 万 | ✗（快捷键被占用） |
| 电脑播放手机的音乐 | 5.0 万 | ✗ |
| 重装系统后不能上网 | 4.8 万 | ✅（「网络通不通」的「网卡没装驱动」教用手机 USB 共享网络先上网） |
| 各浏览器设置主页 | 4.0 万 | 不做（计划书第五节第 9 条） |
| 全屏游戏时仍显示任务栏 | 4.0 万 | ✗ |
| 文件右键菜单太多 | 3.9 万 | ✅ |
| 广告弹窗拦截 | 3.8 万 | ✅ 老弹广告 |
| 设置默认视频播放器 | 3.7 万 | ◐（能打开「默认应用」） |
| 输入法简繁切换 | 3.5 万 | ✗ |

各分类里还有：安装驱动提示没有数字签名 2.6 万、开机黑屏只有鼠标 2.1 万、BIOS 里 VT 被禁用 1.7 万（安卓模拟器要用）、「资源管理器已停止工作」1.6 万、开启了共享文件夹别的设备看不到 2.9 万、微信能上网浏览器打不开 3.1 万、长时间不操作会断网 1.1 万、公共 WiFi 认证页不弹出 0.9 万、U 盘存不了 4G 以上的文件 1.2 万、缩略图不显示 2.3 万、没有睡眠选项 2.3 万、无法固定程序到任务栏 1.7 万、游戏需要安装 DirectPlay 1.7 万、软件卸载了怎么彻底删干净 1.2 万。

**借鉴**：①「上不了网」按用户原话拆出了很多入口（VPN 后上不了网、Ping 传输失败、多个默认网关、重定向次数过多）——我们把这些说法补进搜索关键词；②「DLL、DirectX、运行库」三件套——我们目前只自动装 VC++；③软件搬家有「搬移历史 → 还原」（[帮助](https://guanjia.qq.com/help/rjbj.html)），和我们的修改日志一个思路；④硬件检测能「导出信息」成文本，发给别人求助（我们的「电脑配置」有「复制全部」）。

**不学**：知识库用 AI 批量灌水，两万多篇文章没人看——内容多不等于有用。

### 3.2 360 安全卫士

| 条目 | 类型 | 功能点 |
|---|---|---|
| 系统急救箱（独立版 5.1，2026-07） | 一键 | 扫描启动项、快捷方式、系统关键配置；强力模式查驱动型、MBR 型木马；系统修复把选中项恢复成默认值；网络修复；**DLL 从云端补回**（我们不做）；日志写 SysRepair.log，**不能撤销**（[官网](https://weishi.360.cn/jijiuxiang/)、[说明](http://www.360.cn/privacy/360jijiuxiang.html)） |
| 断网急救箱 | 一键检测 + 一键修复 + 强力修复 | 网络硬件、连接配置、DHCP、DNS、HOSTS、浏览器配置、连通性（[官方](https://weishi.360.cn/work/dwjjx/)） |
| 电脑救援（原「人工服务」） | 一键 + 人工 | 自助工具、免费远程人工救援、收费商家救援 |
| 优化加速 | 一键 | 开机启动项、运行中的软件、服务、右键插件、自启动图标插件、Win10 应用自启动 6 类（[官方](https://weishi.360.cn/clear/yhjs/)） |
| 弹窗过滤、软件净化 | 常驻 | 任务栏闪动、屏幕中间迷你页、右下角广告；截图框住窗口就能过滤 |
| 驱动大师 | 一键 | 云端匹配；**网卡版**（集成万能网卡驱动，没网也能用） |
| 查找大文件、重复文件、文件恢复 | 工具 | 大文件默认跳过 54 类风险文件；重复文件删除前二次确认 |
| 清理 Pro（2025-03） | 一键 | 首页加「软件卸载」；把虚拟内存、休眠文件、系统备份单独列出来（[新闻](https://www.360.cn/n/12666.html)） |
| 开机小助手 | 开机弹出 | 天气、新闻头条、活动（反面教材） |

**借鉴**：①断网急救箱是逐层清单，每层显示通过或不通过——我们的「上不了网」已经按链路查，可以把每一层的结果都亮出来；②C 盘分析把虚拟内存、休眠文件、系统备份单独说明（我们「C 盘被什么占了」已经列出休眠文件和分页文件）；③「没网也能装网卡驱动」的死结——我们不做驱动库，用手机 USB 共享上网解决（已有）。

### 3.3 金山毒霸「电脑医生」

| 条目 | 类型 | 功能点 |
|---|---|---|
| 电脑加速、C 盘瘦身 | 一键 | 清微信、QQ 缓存；专门处理原神、永劫无间这类大型游戏和设计软件的缓存 |
| 声音修复、驱动修复 | 一键 | 耳机没声音、花屏黑屏掉帧 |
| DLL 修复 | 一键 | 号称「上千种 DLL」；V8.1（2025-07）专门处理黑神话这类 DX12 游戏启动缺 DLL |
| 打印机修复 | 一键 | 称收录 95% 的型号、1 万多款驱动 |
| 软件修复 | 一键 | 办公、游戏软件启动失败、闪退、报错 |
| 方案库 + 搜索 | 一键 | 分类 → 方案详情 → 立即扫描 |
| 1V1 专家远程 | 人工（收费转化点） | 黑猫投诉：「一键修不了 → 转人工」涉诉 30 元（[投诉](https://tousu.sina.com.cn/complaint/view/17375043277/)，单一来源） |

来源：[官方页](https://www.ijinshan.com/functions/pcdoctor.html)。金山官网还有大量「xxx.dll 丢失？官方免费修复工具」式文章做搜索引流。

**借鉴**：①用户搜的是**具体的 DLL 文件名**，所以「DLL 名 → 属于哪个运行库或哪个软件」的对照表要做全（第七节 7.5），UnityPlayer、libcef 这类是软件自带的文件，应该重装那个软件；②**软件闪退**：读应用程序事件日志找出出错的模块，对应到运行库、显卡驱动或重装软件；③游戏、聊天软件的缓存专项清理（只清缓存，不动聊天文件）。

### 3.4 火绒安全 6.x

官方手册列了 14 个工具：漏洞修复、系统修复、弹窗拦截、垃圾清理、启动项管理、文件粉碎、右键管理、断网修复、流量监控、修改 HOSTS、ARP 防护、DHCP 检测、安全分析工具、专杀工具（[6.0 用户手册](https://cdn-www.huorong.cn/Public/Uploads/uploadfile/files/20260909/%E7%81%AB%E7%BB%92%E5%AE%89%E5%85%A8%E8%BD%AF%E4%BB%B6%206.0%20%E7%94%A8%E6%88%B7%E6%93%8D%E4%BD%9C%E6%89%8B%E5%86%8C%EF%BC%886.0.12.0%E7%89%88%E6%9C%AC%EF%BC%89.pdf)）。手册只列名称，论坛返回 405，各工具的检测项只能从第三方下载站看到（未核实）。

| 条目 | 功能点 |
|---|---|
| 断网修复 | 网络硬件、连接配置、DHCP、DNS、HOSTS、LSP、IE 代理、**环境变量**（未核实） |
| 启动项管理 | 注册表、服务、计划任务；「保持现状 / 建议删除」；**操作记录里能「恢复原始状态」**（[火绒](https://www.huorong.cn/document/info/productions/119)） |
| 右键管理（6.0.11，2026-06） | **显示每个菜单项是哪个软件加的（带 Logo）**；区分 Win11 一级菜单和「显示更多选项」里的经典菜单；新增「新建」子菜单管控（[火绒](https://www.huorong.cn/document/info/productions/1999)） |
| U 盘保护（常驻） | 插上 U 盘时发现根目录有被隐藏的文件，弹窗问要不要修复（原理：病毒把文件改成「系统 + 隐藏」）（[火绒](https://www.huorong.cn/document/info/productions/1904)） |
| 火绒强力卸载（独立工具，2026-07） | 普通卸载后扫残留，文件和注册表按树形列出可勾选；深度卸载前先查安装目录里正在运行的进程；删不掉的走「重启后删除」（[火绒](https://www.huorong.cn/document/info/productions/2020)） |
| 6.0 其他 | 游戏模式（暂停 Windows 更新、不弹消息）；应用加固新增「AI 软件」一类 |

**借鉴**：①「上不了网」补一项**环境变量**检查（PATH、SystemRoot 被改后 ping、ipconfig 都会失效）；②右键菜单、「新建」菜单显示每一项**是哪个软件加的**；③软件卸载 + 残留清理的流程（先调软件自带卸载 → 扫残留 → 处理占用进程 → 删不掉的重启后删；我们要先备份，保持能撤销）。ARP 防护、DHCP 检测对家庭用户价值低。

### 3.5 微软电脑管家（3.22.6，2026-09）

| 条目 | 类型 | 功能点 |
|---|---|---|
| 一键加速、全面体检 | 一键 | 清临时文件、结束后台进程；「智能加速」要常驻 |
| 存储管理 | 一键 + 工具 | 深度清理（Windows 更新残留等，新版能先关掉占用文件的程序）；大文件按 >10MB/50MB/100MB/1GB 筛选，可删除或**移动到别的盘**；重复文件；磁盘分析（3.14） |
| Windows 功能修复 | 一键 | **任务栏修复**（恢复默认布局）；**重置默认应用**；默认应用被改时主动提示并可一键恢复 |
| 弹窗管理 | 常驻 | 识别弹窗来自哪个应用、拦截记录、截图拦截 |
| 智能圈选 | 工具 | 圈选截图后提取文字、翻译（灰度测试） |
| 龙虾管理（3.20.7，2026-03） | 一键 | OpenClaw 等本地 AI 智能体一键卸载或停止（[腾讯新闻](https://news.qq.com/rain/a/20260317A065ZL00)） |

来源：[官网](https://pcmanager.microsoft.com/zh-cn)、[Windows Central](https://www.windowscentral.com/software-apps/windows-11/what-is-pc-manager-and-how-to-get-started-using-it-on-windows-11)、[Softpedia](https://www.softpedia.com/get/Tweak/System-Tweak/Microsoft-PC-Manager.shtml)。官网没有更新日志页，官方新闻站返回 503。

**借鉴**：①任务栏修复（恢复默认、重启资源管理器）；②默认应用被改：只能恢复默认或跳到设置页（系统不允许程序悄悄改默认应用）；③「大文件移动到别的盘」。

### 3.6 厂商管家

**联想电脑管家（5.1.x）**：2025–2026 年新增 C 盘应用迁移（5.1.120）、右键菜单管理（5.1.140）、**异常报错弹窗处理**（5.1.170，2026-02）、管家智能体（5.1.180）、龙虾安全防护（5.1.190）（[更新日志](https://guanjia.lenovo.com.cn/history.html)）。

**联想 Quick Fix**：知识库里 114 个独立的一键小工具，每个只解决一件事（[工具列表](https://iknow.lenovo.com.cn/tool/lists)）。浏览量前列，**这是目前能拿到的最硬的「国内用户想要什么」数据**：

| 工具 | 浏览量 | 我们 |
|---|---|---|
| 关闭/开启 Win10 自动更新 | 401 万 | 不做「关」（计划书第五节第 5 条）；可以给系统自带的「暂停更新」和「使用时段」的入口 |
| 驱动安装 | 293 万 | 不做驱动库；有「设备和驱动」检测 + Windows 更新可选驱动 + 手机 USB 共享上网 |
| 关闭/开启 Win11 自动更新 | 203 万 | 同上 |
| 关闭 Defender 服务 | 81 万 | 不做（计划书第五节第 4 条） |
| **游戏闪退检测** | 78 万 | ✗ → P1 |
| 电脑高性能管理 | 73 万 | ◐（电源计划） |
| C 盘清理 | 70 万 | ✅ |
| Office 激活白屏修复 | 58 万 | ✗ → P2 |
| 系统文件修复 | 28 万 | ✅ |
| **补丁卸载** | 24 万 | ✗ → P1 |
| 蓝屏分析 | 20 万 | ✅ |
| **局域网共享开启** | 20 万 | ✗ → P0 |
| 共享打印机错误修复 | 17 万 | ✅ |
| IP / DNS / Winsock 重置 | 14 万 | ◐ → P1 |
| 网络图标丢失修复 | 13 万 | ✗ → P1 |
| 软件卸载 | 11 万 | ◐（能打开「已安装的应用」） |
| DPI 修复 | 10 万 | ✗ → P2 |
| **一键创建还原点** | 6.6 万 | ◐（修复前自动建）→ P1 小工具 |
| **网络图标叹号修复** | 6.5 万 | ◐ → P0 |
| **恢复默认文件夹路径** | 6.4 万 | ✗ → P1 |
| **键盘错误代码 19 修复** | 5.7 万 | ✗ → P1 |

**华为电脑管家**：故障排查分一键检测和单项（硬件信息、连接检测、音频检测、卡顿与死机检测、系统软件检测），「立即修复」（[官方](https://consumer.huawei.com/cn/support/pc-manager/)）。**荣耀**：智能检测按硬件、连接、音频、系统软件分板块。**小米**：以跨屏互联为主，没找到故障检测模块。各家分项的具体检测点都抓不到（页面动态加载）。

**借鉴**：①体检的 50 多项按小白能懂的板块（连接、声音、卡顿死机、系统软件、硬件）分组展示；②读出品牌、型号，给该品牌的官方驱动、保修页（零推广）；③「修完做验证」：播放测试音、录音回放。

### 3.7 这一节的结论

1. **需求最旺的单项是「关自动更新」**（联想两个工具合计 600 万浏览）。我们不做「关」，但要把**系统自带的「暂停更新」「使用时段」、更新重启前提醒**放到用户面前，并解释为什么不建议关。
2. **我们最大的缺口**：局域网共享（P0）、网络图标叹号但能上网（P0）、游戏和软件闪退诊断、卸载最近的更新、DirectX / .NET 3.5 / DirectPlay、「键盘代码 19」、恢复默认文件夹路径、一键建还原点、软件卸载和残留。
3. **「每步可撤销」在竞品里只有零星体现**：火绒的启动项「恢复原始状态」、腾讯软件搬家的「还原」；360 急救箱只记日志不能撤销。这是我们的差异点，要守住。
4. **2025–2026 的新方向**：本地 AI 智能体（「龙虾」）管理，微软、联想、火绒都上了；联想的「异常报错弹窗处理」。

---

## 四、国外和开源的修复工具：完整修复项和能借鉴的

### 4.1 FixWin 11（TheWindowsClub，11.2，免费便携、闭源）

官方页面列出 6 类、70 多项修复（[FixWin](https://www.thewindowsclub.com/fixwin-windows-pc-repair-software)）。每项旁边有「?」按钮说明做什么，双击能复制命令；官方建议先跑 SFC、DISM、建还原点，一次只修一项，修完重启。

| 类别 | 修复项（官方原文的中文意思） | 我们 |
|---|---|---|
| 资源管理器（11） | 桌面回收站图标不见、WerMgr/WerFault 报错、文件夹选项缺失或被禁用、回收站图标不刷新、开机后资源管理器不启动、缩略图不显示、回收站损坏、光驱不被识别、「Class not registered」、文件夹选项里没有「显示隐藏文件」、回收站在桌面图标设置里是灰的 | 缺：文件夹选项被禁用、光驱不识别、回收站损坏 |
| 网络（11） | IE 右键菜单被禁用、TCP/IP 损坏、清 DNS、清更新历史、重置防火墙配置、重置 IE、IE 运行时错误、IE 每服务器最大连接数、「Internet 选项」高级页缺失、重置 Winsock、Telnet 不是内部命令 | 有清 DNS；缺 Winsock 重置（`network.yaml` 里已写明还没做） |
| 系统（20） | 组件存储损坏、重置「设置」应用、禁用 OneDrive、开始菜单打不开、升级后 Wi-Fi 不工作、更新卡在下载、多个 OneDrive、沙盒启动失败、更新错误、WSL 注册失败、电池剩余时间不显示、清应用商店缓存、0x8024001e、重新注册全部商店应用、缩略图缓存、重新注册系统 DLL、激活问题、**JPG 被存成 JFIF**、重置虚拟内存、找不到 regedit.exe | 有 DISM+SFC、重置更新组件、虚拟内存、缩略图缓存；缺 JFIF、「设置」和开始菜单打不开、商店缓存 |
| 系统工具（10） | **任务管理器被禁用、CMD 被禁用、注册表编辑器被禁用**、MMC 管理单元被限制、重置 Windows 搜索、**系统还原被禁用**、设备管理器空白、重置 Defender、安全中心不认已装的杀软、重置安全设置 | 有搜索服务；缺「被禁用的系统开关」这一整组 |
| 疑难解答 | 直达 18 个 MSDT 疑难解答 | 其中 8 个已在微软移除清单里，新版 Win11 上会失效（见 4.4） |
| 其他（11 + 21 个重置） | 图标缓存、启用休眠、Aero Snap/Peek/Shake、跳转列表丢失、通知被禁用、WSH 被禁用……；重置组策略、catroot2、WMI 仓库、回收站、Winsock、商店缓存、DNS、TCP/IP、防火墙、设置应用、更新历史、SoftwareDistribution、触摸板、键盘、Edge、**WinHTTP 代理**、字体缓存 | 有图标缓存；缺 WinHTTP 代理残留 |

有官方文档的底层做法（FixWin 本身没公开命令）：

- 任务管理器被禁用：`HKCU\Software\Microsoft\Windows\CurrentVersion\Policies\System\DisableTaskMgr`（[Policy CSP ADMX_CtrlAltDel](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-ctrlaltdel)）；注册表编辑器、CMD（[ADMX_ShellCommandPromptRegEditTools](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-shellcommandpromptregedittools)，值名 DisableRegistryTools、DisableCMD 官方表里没列，未核实）；文件夹选项 NoFolderOptions（[ADMX_WindowsExplorer](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-windowsexplorer)）。**这些都是策略，按我们的规则只检测、说明，不替用户删**。
- 光驱不被识别：删 `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4D36E965-E325-11CE-BFC1-08002BE10318}` 的 UpperFilters、LowerFilters 后重启（[微软支持](https://support.microsoft.com/en-us/topic/your-cd-or-dvd-drive-is-not-recognized-by-windows-or-other-programs-64da3690-4c1d-ef04-63b8-cf9cc38ca53e)）。
- Winsock：`netsh winsock reset`；`netsh winsock show catalog` 能列出 LSP（[netsh winsock](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netsh-winsock)）。
- WinHTTP 代理：先 `netsh winhttp show proxy` 记下来，再 `reset proxy`（[netsh winhttp](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netsh-winhttp)）。它和我们已经查的系统代理（WinINet）是两套设置，**Windows 更新、应用商店走的是 WinHTTP**。
- WMI：`winmgmt /verifyrepository`，坏了再 `/salvagerepository`；`/backup`、`/restore` 能撤销（[winmgmt](https://learn.microsoft.com/en-us/windows/win32/wmisdk/winmgmt)）。
- 「设置」应用：`Reset-AppxPackage`（[Reset-AppxPackage](https://learn.microsoft.com/en-us/powershell/module/appx/reset-appxpackage)）。
- JFIF：把 `HKCR\MIME\Database\Content Type\image/jpeg` 的 Extension 改回 `.jpg`（二手来源，[TheWindowsClub](https://www.thewindowsclub.com/windows-10-saving-jpgs-downloaded-from-the-internet-as-jfifs)）。

**借鉴**：①「被禁用的系统开关」做成一项检测（见第八节）；②每一步都能看到将执行的命令、能复制（我们的预览已经列出改动，可以再加「复制」）；③WMI 自检放进启动自检，因为几十项检测都靠 WMI。

### 4.2 Tweaking.com Windows Repair（4.14.0，2023-06；修复部分免费，闭源）

流程值得学：断电重置 → 预扫描（包文件、联接点、环境变量）→ **chkdsk 先只读扫描，发现问题才建议重启修** → SFC；修复前默认备份注册表；5 个预设（[官网](https://www.tweaking.com/features/windows-repair-all-in-one/)、[更新日志](https://www.tweaking.com/tweaking-com-windows-repair-change-log/)）。

33 项修复（编号来自二手博客，名称和官方更新日志能互相印证）：1–3 重置注册表/文件/服务权限（**计划书第五节第 28 条：不做**）；4 注册系统文件；5 修 WMI；6 修防火墙；9 hosts（原文件备份）；10 删除病毒留下的策略（禁用任务管理器、禁用 Defender、隐藏桌面、IFEO 劫持）；11 还原被藏起来的开始菜单快捷方式；12 图标缓存；13 网络（DNS、Winsock、TCP/IP，以及 netsh int 下的 6to4、ipv4、ipv6、httpstunnel、isatap、portproxy、tcp、teredo）；14–15 临时文件、代理；16 Windows 更新（重置组件、BITS、pending.xml 改名）；17 光驱 Upper/LowerFilters；18 VSS；20 MSI（`msiexec /unregister`、`/regserver`）；21 截图工具；22 文件关联（含 .lnk）；23 安全模式 SafeBoot 键；24 打印后台（删 spool\PRINTERS）；25–26 服务恢复默认启动类型（按 BlackViper 基线）；27 应用商店重新注册；28 组件存储；29 COM Unmarshalers（`HKCR\Unmarshalers\System`，丢了音频、Defender、WMI 都会坏）；30「新建」子菜单；31 UAC 默认值；32 性能计数器 `lodctr /r`；33 回收站（会清空）。

**借鉴**：①chkdsk「先只读、有问题再修」；②第 25、26 项只取「关键服务被禁用」的检测和恢复，不做全量重置；③第 17 项的思路推广到键盘类 `{4D36E96B-…}`：UpperFilters 里应该只有 kbdclass，被别的软件改了会出 Code 19/39（社区，未核实）；④MSI 修复、性能计数器（任务管理器性能页空白）；⑤第 31 项 UAC 默认值是 DENIED_VALUES，我们只检测。

### 4.3 其他工具箱和调整器

- **Windows Repair Toolbox**（3.0.4.8，2025-09，闭源）：本质是第三方便携工具的下载启动器；**最值得学的是「修完做验证」**：扬声器、键鼠、摄像头麦克风测试（[官网](https://windows-repair-toolbox.com/)）。我们工具箱里已经有这些测试，只差在症状修完以后把它们链过去。
- **Ultimate Windows Tweaker 5**（5.2，200 多项，闭源）：Security 区（禁用注册表编辑器、控制面板、任务管理器、CMD、文件夹选项、MMC、右键菜单、关机）正好是「被禁用的系统开关」检测清单；性能区的「禁用 Superfetch、索引、打印后台」正是我们要查出来、改回去的「优化后遗症」（[UWT5](https://www.thewindowsclub.com/ultimate-windows-tweaker-5-for-windows-11)）。
- **Winaero Tweaker**（1.65，2026-02，EULA 禁止再分发）：分类可参考；它的禁 Defender、禁更新、禁 UAC 违背我们的原则（[Winaero](https://winaero.com/winaero-tweaker/)）。

### 4.4 微软「获取帮助」和系统自带的恢复能力

**Windows 11 现在的 10 个自动疑难解答**，都用 `ms-contact-support://smc-to-emerald/<名字>` 调起（[微软：Windows 疑难解答](https://support.microsoft.com/en-us/support/get-help/windows-troubleshooters)）：

| 名字 | 疑难解答 | 接到我们的症状 |
|---|---|---|
| AudioTroubleshooter | 音频 | 没声音 |
| BITSTroubleshooter | BITS | 更新失败 |
| BluetoothTroubleshooter | 蓝牙 | 蓝牙 |
| TroubleshootCamera | 相机 | 麦克风、摄像头 |
| NetworkAndInternetTroubleshooter | 网络和 Internet | 上不了网 |
| PrinterTroubleshooter | 打印机 | 打印机 |
| ProgramCompatTroubleshooter | 程序兼容性 | 新症状「老软件打不开」 |
| VideoPlaybackTroubleshooter | 视频播放 | — |
| WMPTroubleshooter | Windows Media Player | — |
| WUTroubleshooter | Windows 更新 | 更新失败 |

- **MSDT 退役**：2023 年开始重定向到「获取帮助」，2024 年移除其余，2025 年移除平台；只影响 Win11 22H2 之后的版本，Win10 和更早的 Win11 照旧（[MSDT 弃用](https://support.microsoft.com/en-us/windows/deprecation-of-microsoft-support-diagnostic-tool-msdt-and-msdt-troubleshooters-0c5ac9a2-1600-4539-b9d0-069e71f9040a)）。移除清单 14 个：DirectAccess、设备和打印机、硬件和设备、家庭组、传入连接、IE 性能、IE 安全、键盘、电源、搜索和索引、语音、系统维护、共享文件夹、应用商店应用。
- Win10 上用 `msdt.exe /id <包名>`：AudioPlaybackDiagnostic、AudioRecordingDiagnostic、NetworkDiagnosticsWeb、NetworkDiagnosticsNetworkAdapter、NetworkDiagnosticsFileShare、PrinterDiagnostic、WindowsUpdateDiagnostic、DeviceDiagnostic、PowerDiagnostic、SearchDiagnostic、PCWDiagnostic……（Win7/8 时代的表，[来源](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/ee424379(v=ws.11))，Win10 上要实机验证）。msdt 的返回码：0 已修复、1 发现问题没修好、2 没发现问题、-1 中途关闭（[msdt](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/msdt)）。**不用 `ms-msdt:` 协议**：它就是 Follina 漏洞的入口。
- 离线能不能用、中国区有没有限制：**查不到官方说明**。「获取帮助」应用缺失时会提示「无法打开此 ms-contact-support 链接」（[微软问答](https://learn.microsoft.com/en-us/answers/questions/2288265/how-to-reinstall-get-help-app)）。

| 系统自带的恢复能力 | 条件 | 会不会动个人文件 | 怎么打开 |
|---|---|---|---|
| 用 Windows 更新修复问题（重装当前版本） | Win11 22H2 以上、装了 2024-02 的可选更新；单位管理的电脑没有；要联网 | 保留应用、文件、设置 | `ms-settings:recovery` →「立即重新安装」（[微软](https://support.microsoft.com/en-us/windows/fix-issues-by-reinstalling-the-current-version-of-windows-497ac6da-7cac-4641-82a5-f50398d879a0)） |
| 快速机器恢复 | 24H2（26100.4700）以上，家庭版和个人的专业版默认开 | 不重置 | `reagentc /getrecoverysettings`（[QMR](https://learn.microsoft.com/en-us/windows/configuration/quick-machine-recovery/)） |
| 时间点还原 | 24H2 以上；个人电脑、系统盘 ≥200GB 默认开；每 24 小时一个点，保留 72 小时，最多占 2%（2–50GB） | **整盘回滚，个人文件也会回去** | 只能在恢复环境里发起，要 BitLocker 恢复密钥（[PITR](https://learn.microsoft.com/en-us/windows/configuration/point-in-time-restore)） |
| 回退到上一版本 | 升级后 10 天内 | 保留文件，删掉升级后装的应用 | `ms-settings:recovery` |

**借鉴**：①每个症状页加一个「微软官方诊断」按钮（先查有没有「获取帮助」应用，Win11 23H2 以上用 URI，更早的用 msdt 读返回码，打不开就回到我们自己的检查）；②「蓝屏」「更新以后出问题」的最后放一张官方兜底卡片（重装当前版本）；③讲时间点还原时和我们的「BitLocker 恢复密钥」联动。

### 4.5 PowerToys（MIT，0.101，31 个模块）

| 模块 | 对小白 | 我们用系统自带能力怎么做 |
|---|---|---|
| Text Extractor | 高 | 图片转文字：`Windows.Media.Ocr`，官方说只能在 Windows PowerShell 里用（正好是我们的环境）；缺中文识别包时引导装（`Get-WindowsCapability` 查 `Language.OCR*`，中文包名未核实）（[Text Extractor](https://learn.microsoft.com/en-us/windows/powertoys/text-extractor)、[OcrEngine](https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr.ocrengine)） |
| Keyboard Manager | 中高 | PowerToys 要常驻；我们写 `HKLM\SYSTEM\CurrentControlSet\Control\Keyboard Layout\Scancode Map`（二进制，重启生效，删掉再重启就恢复），只做键对键：坏键换成别的键、禁用 Win 键、CapsLock 和 Ctrl 互换（[键盘类驱动](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/keyboard-and-mouse-class-drivers)） |
| Hosts File Editor | 已有 | 学它编辑前自动备份（带时间戳，保留最近 5 份） |
| File Locksmith、Image Resizer、PowerRename、Awake、New+ | 已有 | — |
| Environment Variables | 中 | 只做 PATH 体检：系统 PATH 里没了 System32 时恢复（先备份原值） |
| Power Display | 中 | 外接显示器亮度：DDC/CI 的 SetMonitorBrightness，做成一次性的滑块，不常驻 |
| Mouse utilities、ZoomIt | 中（老人） | 系统自带的「按 Ctrl 显示指针位置」（SPI_SETMOUSESONAR）、`ms-settings:easeofaccess-mousepointer`、放大镜 `ms-settings:easeofaccess-magnifier` |
| 其余 16 个 | 低 | 都要常驻热键或钩子，违背「不常驻」 |

### 4.6 NirSoft 和 Sysinternals：适合做成「只看不改」小工具的

| 工具 | 读的是什么 | 我们怎么做 |
|---|---|---|
| TurnedOnTimesView | 系统日志 Kernel-Power 41、42（睡眠）、Power-Troubleshooter 1（唤醒）、USER32 1074（谁发起的关机）、EventLog 6005/6006 | **开关机记录**：再加 6008（异常关机）；按 41 的字段分蓝屏（BugcheckCode≠0）、长按电源键（PowerButtonTimestamp≠0）、断电（全 0）（[事件 41](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/event-id-41-restart)）。「最近的蓝屏」已经用了 41 和 1074，扩成完整的时间线 |
| WifiHistoryView、WifiInfoView | WLAN-AutoConfig/Operational 日志 8001/8002/8003/11001；Native Wi-Fi API | **WiFi 断线记录**；附近 WiFi 的信道和信号（Win11 24H2 起要打开定位权限） |
| DevManView | 设备属性 | 已有「设备和驱动」检测，补全错误码的中文说明（第七节 7.3） |
| ShellExView | 注册的外壳扩展 | 已有右键菜单管理；加「逐个关掉找出右键卡顿的元凶」 |
| WhatIsHang、AppCrashView | 无响应窗口、WER 报告 | **程序崩溃和卡死记录**：Win32_ReliabilityRecords（已有「最近的崩溃和蓝屏」检测，扩成小工具）+ Application 1002 |
| NetworkUsageView | SRUM 数据库 `System32\sru\SRUDB.dat` | **哪个程序偷跑流量**：要读 ESE 数据库，复杂，P2 |
| WinUpdatesView | Windows Update API | 已有「最近的更新」检测，读 QueryHistory 拿失败的 KB 和错误码 |
| TaskSchedulerView | 全部计划任务 | **计划任务里的非微软、隐藏任务**：弹窗、半夜开机的常见元凶，接到「弹广告」「自己开机」 |
| Autoruns | 各种自启动位置 | 启动项列表已经有签名；再查 IFEO 劫持（只检测）和 Winsock LSP（`netsh winsock show catalog`） |
| DiskSmartView | S.M.A.R.T. | 已有硬盘健康，按 CrystalDiskInfo 的规则细化（5.9） |
| Streams | 备用数据流 | 只处理 Zone.Identifier：「下载的文件被阻止」用 `Unblock-File` 解除，要给安全提示 |

许可：NirSoft 可以免费原样分发但不是开源；Sysinternals 的许可禁止再发布（[Sysinternals 许可](https://learn.microsoft.com/en-us/sysinternals/license-terms)）。两者都只借思路，自己实现。

### 4.7 实现时要注意的三件事

1. **24 小时内只能建一个还原点**：Checkpoint-Computer 会跳过，SRSetRestorePoint 甚至返回「成功」却给旧序号（[Checkpoint-Computer](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/checkpoint-computer?view=powershell-5.1)）。我们的 `scripts/host/restore-point.ps1` 前后数一次还原点，跳过时如实告诉用户（已核对，没问题）。微软给开发者的官方开关是临时把 `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore` 的 SystemRestorePointCreationFrequency 设为 0（[SRSetRestorePoint](https://learn.microsoft.com/en-us/windows/win32/api/srrestoreptapi/nf-srrestoreptapi-srsetrestorepointw)），做完改回原值，这样每次「谨慎」级别的修复前都有一个新鲜的还原点。
2. WMIC 已经从 Win11 24H2 以上移除（[已弃用功能](https://learn.microsoft.com/en-us/windows/whats-new/deprecated-features)），我们的脚本本来就只用 CIM（已核对）。
3. SRUM、SMART、pnputil、MFT 这些读取都要管理员（小药箱本来就以管理员运行）。

---

## 五、GitHub 上能直接复用的数据和代码

Star 数、许可证、最近推送时间来自 GitHub 搜索 API（2026-09-28）；数据文件都从 raw.githubusercontent.com 下载原文逐条核对，文中「L 行号」是当天默认分支上的行号。

### 5.1 许可证总表（我们是 GPL-3.0）

| 项目 | 许可证 | ★ / 状态 | 能不能并进来 |
|---|---|---|---|
| [ChrisTitusTech/winutil](https://github.com/ChrisTitusTech/winutil) | MIT | 6.3 万 / 活跃（2026-09-26） | 能 |
| [Raphire/Win11Debloat](https://github.com/Raphire/Win11Debloat) | MIT | 5.8 万 / 活跃 | 能 |
| [farag2/Sophia-Script-for-Windows](https://github.com/farag2/Sophia-Script-for-Windows) | MIT | 9.8k / 活跃（7.3.0，2026-09） | 能 |
| [Disassembler0/Win10-Initial-Setup-Script](https://github.com/Disassembler0/Win10-Initial-Setup-Script) | MIT | 4.6k / 2021 年归档 | 能 |
| [Chuyu-Team/Dism-Multi-language](https://github.com/Chuyu-Team/Dism-Multi-language)（Data.xml） | MIT | 2.0 万 / 规则 2023-03 后没改过 | 能 |
| [hiyohiyo/CrystalDiskInfo](https://github.com/hiyohiyo/CrystalDiskInfo) | MIT | 3.4k / 活跃 | 能 |
| [blackboxo/CleanMyWechat](https://github.com/blackboxo/CleanMyWechat) | MIT | 5.5k / 2026-08 还在更新 | 能 |
| [BCUninstaller/Bulk-Crap-Uninstaller](https://github.com/BCUninstaller/Bulk-Crap-Uninstaller) | Apache-2.0 | 2.2 万 / v6.3（2026-09） | 能（保留 NOTICE、声明修改） |
| [MoscaDotTo/Winapp2](https://github.com/MoscaDotTo/Winapp2) | CC BY-SA 4.0 | 1.0k / 活跃 | 能（单向兼容 GPLv3：署名、写明修改和日期） |
| [rcmaehl/WhyNotWin11](https://github.com/rcmaehl/WhyNotWin11) | LGPL-3.0 | 6.4k | 能 |
| [bleachbit/bleachbit](https://github.com/bleachbit/bleachbit)（CleanerML） | GPL-3.0-or-later | 7.0k | 能（保留版权头） |
| [microsoft/PowerToys](https://github.com/microsoft/PowerToys) | MIT | — | 能（我们只借思路，用系统自带能力实现） |
| [undergroundwires/privacy.sexy](https://github.com/undergroundwires/privacy.sexy) | AGPL-3.0 | 6.1k / 放缓 | **只取事实**：注册表路径和值是事实不受保护，说明文字和编排受保护；组合进来的那部分仍是 AGPL |
| [lostindark/DriverStoreExplorer](https://github.com/lostindark/DriverStoreExplorer) | GPL-2.0（没找到「或更高版本」声明） | 1.2 万 / 活跃 | **不能合代码**，只借思路重写 |
| FluentTweaker、ZyperWinOptimize | 没有许可证 | — | 不能用 |

落地：新建 `THIRD_PARTY_NOTICES`，每个改编来的数据文件头写「部分规则改编自 ×××（许可证），已修改，日期」。

### 5.2 winutil（MIT）：67 项调整里对小白有用、我们还没有的

数据：[config/tweaks.json](https://github.com/ChrisTitusTech/winutil/blob/main/config/tweaks.json)，每项是 `{registry[{Path,Name,Type,Value,OriginalValue}], service[{Name,StartupType,OriginalType}], InvokeScript/UndoScript}`。

| 条目（行） | 改什么 | 价值 | 风险 |
|---|---|---|---|
| 任务栏右键「结束任务」L855 | `HKCU\…\Explorer\Advanced\TaskbarDeveloperSettings` TaskbarEndTask=1 | 程序卡死时一键关 | 低（22631 起） |
| 蓝屏显示参数 L1263 | `HKLM\SYSTEM\CurrentControlSet\Control\CrashControl` DisplayParameters=1 | 拍照求助时能看到参数 | 低 |
| 滚动条常显 L1465 | `HKCU\Control Panel\Accessibility` DynamicScrollbars=0 | 老人找不到滚动条 | 低 |
| 开机 NumLock L1555 | `HKU\.DEFAULT\Control Panel\Keyboard` 和 `HKCU\…\Keyboard` InitialKeyboardIndicators=**字符串** "2" | 登录时小键盘不亮 | 低 |
| 关拖动贴靠 L1581 | `HKCU\Control Panel\Desktop` WindowArrangementActive=字符串 "0" | 窗口老被吸到屏幕边 | 低 |
| MPO L1483 | `HKLM\SOFTWARE\Microsoft\Windows\Dwm` OverlayTestMode=5 | 屏幕闪烁、黑块 | 中，只在症状里给 |
| 长路径 L1858 | `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem` LongPathsEnabled=1 | 解压深层目录报错 | 低 |
| IPv4 优先 L1132 | `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters` DisabledComponents=0x20 | IPv6 半通时网页慢 | 中：和「禁 IPv6」（0xFF）写的是同一个值，互斥 |
| 系统修复 Invoke-WPFSystemRepair.ps1 | `chkdsk /scan /perf`（退出码 1 已修复、2 已清理或需要 /f、3 失败）→ `sfc /scannow` → `DISM /RestoreHealth`（3010 需重启） | 我们的「检查并修复系统文件」可以补上退出码的中文说明 | — |
| 网络重置 Invoke-WPFFixesNetwork.ps1 | `netsh winsock reset`；`netsh int ip reset` | 断网的最后一级 | 中：要重启、撤销不了 |
| 注册表定期备份（feature.json L94） | `…\Session Manager\Configuration Manager` EnablePeriodicBackup=1 | 撤销之外多一层保险 | 低 |

**不要照搬**：它的「Windows 更新重置」（Invoke-WPFFixesUpdate.ps1 L140-154）会删掉整棵 `HKLM/HKCU\Software\Policies`、用 secedit 恢复默认、删 GroupPolicy 目录——我们的「重置更新组件」更保守，保持；遥测项里顺带改 Defender；BitLocker 解密放在「基本」里；服务项把 SharedAccess（热点、网络共享要用）设成禁用；磁盘清理带 `/ResetBase`（之后不能卸载更新）；鼠标加速 L1521、粘滞键 L1750 把字符串值写成了 DWORD。

### 5.3 Win11Debloat（MIT）：103 项里我们缺的

数据：[Config/Features.json](https://github.com/Raphire/Win11Debloat/blob/master/Config/Features.json)（每项带 MinVersion/MaxVersion，能直接对应我们的 `applies_to`）、`Regfiles/` 和 `Regfiles/Undo/`。

| 条目（行） | 改什么 | 价值 |
|---|---|---|
| 关贴靠布局 L907 | Adv EnableSnapBar=0、EnableSnapAssistFlyout=0 | 鼠标碰到屏幕顶就弹布局 |
| Alt+Tab 不列 Edge 标签 L921 | Adv MultiTaskingAltTabFilter=3 | Alt+Tab 被标签页淹没 |
| 点任务栏回到上次的窗口 L1140 | Adv LastActiveClick=1 | 多窗口程序 |
| U 盘在侧边栏出现两次 L1338 | 删 `HKLM\…\Explorer\Desktop\NameSpace\DelegateFolders\{F5FB2C77-0E2F-4A16-A381-3E560C68BC83}`（撤销：重建，默认值 "Removable Drives"） | 接到「资源管理器多余图标」 |
| 盘符显示在前面 L1612 | Exp ShowDriveLettersFirst=4 | 电话指导「打开 D 盘」 |
| Win11「此电脑」显示文档、下载等文件夹 L1515 | `HKLM\…\MyComputer\NameSpace\{…}` 的 HiddenByDefault=0 | 老用户找不到文件夹 |
| 开始菜单不推「手机连接」L706 | `HKCU\…\Start\Companions\Microsoft.YourPhone_8wekyb3d8bbwe` IsEnabled=0 | 推广 |
| 桌面「了解此图片」图标 L528 | `Exp\HideDesktopIcons\NewStartPanel` {2cc5ca98-6485-489a-920e-b3e88a6ccce3}=1 | 推广 |
| 拖文件时顶上不冒分享栏 L824 | `HKCU\…\CDP` DragTrayEnabled=0 | 26200 起 |
| 建议类里我们缺的 L432 | CDM SubscribedContent-338388Enabled、SystemPaneSuggestionsEnabled、SubscribedContent-353698Enabled=0；`…\SystemSettings\AccountNotifications` EnableAccountNotifications=0；Adv Start_AccountNotifications=0；`…\Notifications\Settings\Windows.SystemToast.Suggested`、`…BackupReminder` Enabled=0 | 补进「推荐和广告」那一组 |

默认删除的 84 个应用不移植（计划书第五节第 6 条），其中便笺、闹钟、快速助手（远程帮忙常用）国内用户也在用。

### 5.4 Sophia Script（MIT）：120 个函数里我们缺的

数据：[Module/Sophia.psm1](https://github.com/farag2/Sophia-Script-for-Windows/blob/main/src/Sophia_Script_for_Windows_11/Module/Sophia.psm1)。中文本地化只有运行时提示语，函数说明是英文和俄文，下面是我们自己的译法。

| 函数（行） | 改什么 | 价值 |
|---|---|---|
| WindowsManageDefaultPrinter L4416 | `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Windows` LegacyDefaultPrinterMode=1 | **默认打印机老被换成最后用的那台**（打印机症状） |
| NetworkAdaptersSavePower L5225 | 对已连接的物理网卡 `Set-NetAdapterPowerManagement` AllowComputerToTurnOffDevice=Disabled | **WiFi、网线隔一会儿就断**（逐块网卡快照） |
| RecycleBinDeleteConfirmation L1684 | Exp ShellState（二进制）第 5 字节的 0x04 位 | 删文件前确认，防误删（位操作，快照存整值） |
| FolderGroupBy L3228 | `Exp\FolderTypes\{885a186e-a440-4ada-812b-db871b942259}\TopViews\…` | 「下载」按日期分组，找不到文件 |
| SearchHighlights L2060 | `HKCU\…\SearchSettings` IsDynamicSearchBoxEnabled=0 | 搜索框里的热点涂鸦 |
| F1HelpPage L5932 | `HKCU\Software\Classes\Typelib\{8cec5860-07a1-11d9-b15e-000d56bfe6ee}\1.0\0\win64` 默认值 "" | 误按 F1 打开浏览器 |
| ControlPanelView L2499 | `Exp\ControlPanel` AllItemsIconView=0、StartupPage=1 | 远程指导时界面一致 |
| AppsLanguageSwitch L2889 | `Set-WinLanguageBarOption -UseLegacySwitchMode` | 每个窗口单独记住输入法 |
| SaveRestartableApps L6251 | `HKCU\…\Winlogon` RestartApps=0 | 开机自动弹出一堆软件 |
| RestartNotification / ActiveHours L4935 / L5047 | `HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings` RestartNotificationsAllowed2=1 / SmartActiveHoursState=1 | 更新重启前提醒、自动活动时间（不禁更新） |

**不要照搬**：TempTask 里「C:\Recovery 里有 ReAgentOld.xml 时删掉整个 C:\Recovery」（L8751-8753）；PUAProtection 用的是 Set-MpPreference（我们的 lint 禁止改 Defender 设置）。

### 5.5 Win10-Initial-Setup-Script（MIT，已归档）

457 个函数，能用的事实：蓝屏后不自动重启 `…\CrashControl` AutoReboot=0（L1781）；**自动维护半夜叫醒电脑** `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance` WakeUp=0（L1491，补上「自己开机」的另一个来源）；以管理员身份运行的软件看不到映射的网络盘 EnableLinkedConnections=1（L855，在 Policies\System 下，只检测）；删掉「传真」打印机 `Remove-Printer -Name "Fax"`（L3913，打印总跑到传真）；标题栏显示完整路径 `Exp\CabinetState` FullPath=1（L2467）。陷阱：NumLock（L2303）把字符串写成了 DWORD。

### 5.6 privacy.sexy（AGPL-3.0，只取事实）

[windows.yaml](https://github.com/undergroundwires/privacy.sexy/blob/master/src/application/collections/windows.yaml)：189 类、920 个脚本。可以参考的事实：零碎日志的位置（`%SYSTEMROOT%\Temp\CBS`、`Logs\waasmedic`、`Logs\NetSetup`、setupact/setuperr、`WinSAT\winsat.log`），但 CBS.log、DISM.log、Panther 是排错证据，**先诊断再清**。不要：关 Defender、关更新、hosts 屏蔽、清事件日志、卷影副本、SoftwareDistribution、WebCache、Prefetch，以及 L2631 的 DisableResetbase=0（之后的更新没法卸载）。

### 5.7 Dism++ 的 Data.xml（MIT，规则 2023 年后停更）

73 条清理规则 + 21 组中文命名的优化项。Level 0–3，数字越大越安全：0 存在风险（新手模式不显示）、1 建议保留、2 可以删除（默认不清）、3 建议删除（默认清），不写时是 2（[规则结构参考](https://github.com/Chuyu-Team/Dism-Multi-language/blob/master/www.chuyu.me/zh-Hans/library/Dism%2B%2BLibrary/%E8%A7%84%E5%88%99%E7%BB%93%E6%9E%84%E5%8F%82%E8%80%83.md)）。

- **好套路**：WPS、360 浏览器、酷狗、2345 拼音、PPLive、阿里旺旺的「旧版本号目录」规则（L290-306 等）：从注册表读安装位置，删 `*.*.*.*` 这类旧版本目录，保留当前版本。
- **实用**：常见驱动解压目录 `C:\AMD`、`C:\Intel`、`C:\NVIDIA`（L906）。
- **别搬**：「腾讯相关软件下载目录」（L986-999，Level 3）其实删的是整个 `%ProgramData%\temp`；「Windows 日志」（L822）太激进；关闭打开程序的安全警告（L2333）；隐藏快捷方式小箭头（L3039，指向 Blank.ico，文件缺了图标变黑块）。
- **中文优化项**：微软拼音 `HKCU\Software\Microsoft\InputMethod\Settings\CHS` 下 "Default Mode"=1（默认英文，L6442）、"Enable Cloud Candidate"=0（关云候选，L6484）、"Enable Fuzzy Input"=1（模糊音，L6522）——我们已经在这个键写「Ctrl 切换中英文」，可以再加这三项。
- **右键菜单常见项清单**（带现成中文名，给我们的右键菜单管理预置说明）：兼容性疑难解答、以便携式方式打开 {D6791A63-…}、还原以前版本 {596AB062-…}、刻录到光盘 {fbeb8a05-…}、固定到快速访问、Intel 集显、NVIDIA 控制面板（L4815-5759）。只取清单，不取它「改键名」的做法。

### 5.8 Winapp2（CC BY-SA 4.0）：**有一条会删光 QQ 聊天记录**

[Winapp2.ini](https://github.com/MoscaDotTo/Winapp2/blob/master/Winapp2.ini)（版本 260915，4,068 条）。

- **`[Tencent QQ *]`（L30573-30577）有一行 `FileKey2=%UserProfile%\Documents\Tencent Files\|*\|RECURSE`：递归删掉整个「文档\Tencent Files」，里面是聊天记录和收到的文件。绝对不能照搬。**
- 三个版本都**没有**微信 4.x（xwechat_files）、QQ NT（nt_qq）、钉钉、百度网盘、迅雷、腾讯视频、爱奇艺、网易云、QQ 音乐、剪映、抖音、飞书、腾讯会议。
- 能用的：`[Tencent WeChat *]` 里 3.x 的日志（L30579-30588）、QQ 浏览器、360 浏览器这类 Chromium 模板化的缓存规则。WPS 的 `backup\*` 可能是文档备份，只取日志和缓存。

### 5.9 CrystalDiskInfo（MIT）：硬盘健康的判定规则

[AtaSmart.cpp](https://github.com/hiyohiyo/CrystalDiskInfo/blob/master/AtaSmart.cpp) 的 CheckDiskStatus（L12522 起）：

- **机械硬盘**：05 重新分配扇区数、C5 待映射扇区数、C6 不可校正扇区数的**原始值 ≥ 1 就是「警告」**（默认阈值都是 1，DiskInfoDlgInit.cpp L441-444）；原始值是 FFFFFFFF 时忽略；温度 C2 不参与判定；关键属性当前值低于厂商阈值是「不良」。
- **固态硬盘**：任一属性当前值低于阈值是「不良」；剩余寿命按厂商取不同属性（A9、AD、B1〔三星〕、CA、D1、E7〔金士顿、海力士、长江存储等〕……），=0 是不良，≤10% 是警告。
- **NVMe**：严重警告（01）不为 0 是不良；可用备用空间低于阈值是不良；寿命（100 − 已用百分比）≤10% 是警告；型号以 VMware、QEMU、Parallels 开头的算「未知」。
- 中文属性名直接能用（[Simplified Chinese.lang](https://github.com/hiyohiyo/CrystalDiskInfo/blob/master/Language/Simplified%20Chinese.lang)）：05 重新分配扇区数、C5 有待处置扇区数、C6 不可校正的扇区数、C7 UltraDMA CRC 错误计数（和数据线、接口有关）；NVMe 01 严重警告标志、03 可用备用空间、05 已用寿命百分比、0E 介质与数据完整性错误计数。状态词是「良好 / 警告 / 不良」。

我们的 `smart-health.ps1` 现在只用 Get-PhysicalDisk 和 StorageReliabilityCounter。机械盘可以读 `root\wmi` 的 MSStorageDriver_FailurePredictData 和 FailurePredictThresholds 套上面的规则；NVMe 的健康日志要用 IOCTL_STORAGE_QUERY_PROPERTY，适合放进 Rust 引擎。

### 5.10 BCUninstaller（Apache-2.0）：卸载残留的打分规则

[ConfidenceRecords.cs](https://github.com/BCUninstaller/Bulk-Crap-Uninstaller/blob/master/source/UninstallTools/Junk/Confidence/ConfidenceRecords.cs)：是卸载信息里登记的 +20；公司名对上、子目录都对上、明确关联、空文件夹各 +4；产品名完全一致、没有子目录各 +2；直接在已知文件夹里 −1；公司名对不上、名字等于公司名、名字只是勉强像、文件超过 100 个、被名字相近的软件用着各 −2；目录名可疑（install、settings、config、data……）−3；有 exe、程序名或发布者还被别的软件用着各 −4；目录还在用 −7；是商店应用 −10。求和分档：<0 坏、0–1 有问题、2–4 好、≥5 非常好。

**要注意**：国内软件的显示名是中文（「微信」）、目录是英文（Tencent\WeChat），按名字相似度基本对不上，要改用 InstallLocation、Publisher 加一张中英名称对照表；默认只展示「非常好」的，并且进回收站。

### 5.11 DriverStoreExplorer（GPL-2.0，只借思路）：清理旧驱动包

判定（[DSEForm.cs L842-898](https://github.com/lostindark/DriverStoreExplorer/blob/master/Rapr/DSEForm.cs)）：排除启动关键驱动和 ntprint.inf；按（类别、提供商、原始 INF 名）分组，组内按版本、日期降序；只有一个版本的跳过；最新版保留；其余版本里**没有绑定任何设备**的才选中。我们自己实现：`pnputil /enum-drivers` 列出 → 删之前 `pnputil /export-driver` 导出备份 → `/delete-driver oemNN.inf`（**不加 /force**，被占用就失败）→ 撤销用 `/add-driver`（[pnputil](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/pnputil-command-syntax)）。显卡驱动一次好几百 MB，旧版本常攒下几个 GB。

### 5.12 WhyNotWin11（LGPL-3.0）、builtbybel

- WhyNotWin11：CPU 支持表（Intel 1,060 行、AMD 398 行、高通 25 行，表头 2025.10.6）；读 Windows 自己的升级评估 `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\TargetVersionUpgradeExperienceIndicators\<目标版本>` 的 RedReason（CpuFms、Tpm、UefiSecureBoot 就是对应项不满足）——可以给我们的「能不能升级 Win11」做兜底解释。
- Flyoobe（MIT）：`repair-built-in-apps` 对所有已装的应用包 `Add-AppxPackage -DisableDevelopmentMode -Register AppxManifest.xml`，修「设置、计算器打不开」。
- CrapFixer（MIT）：右键菜单里「用照片编辑」{BFE0E2A4-C70C-4AD7-AC3D-10D1ECEBB5B4}、「用 Clipchamp 编辑」{8AB635F8-9A67-4698-AB99-784AD929F3B4}、「询问 Copilot」{CB3B0003-8088-4EDE-8769-8B354AB2FF8C} 的 CLSID（写进 Shell Extensions\Blocked 就隐藏）。它的「快速关机」（WaitToKillAppTimeout=2000）会丢未保存的文档，不搬。

### 5.13 中文项目

| 仓库 | 许可证 / ★ | 能用的 | 评价 |
|---|---|---|---|
| [CleanMyWechat](https://github.com/blackboxo/CleanMyWechat) | MIT / 5.5k | 微信 3.x、4.x、企业微信的目录结构；按天数筛选；删到回收站 | **清理微信的首选依据** |
| [niuhai/skill-c-cleaner](https://github.com/niuhai/skill-c-cleaner) | MIT / 322 | `app-signatures.json`：钉钉 `%APPDATA%\DingTalk`、飞书、网易云 `%LOCALAPPDATA%\Netease\CloudMusic`、百度网盘 `%APPDATA%\baidunetdisk`、夸克、迅雷、搜狗输入法……哪些子目录可清、哪些勿动；还有一份捆绑软件清单 | 路径粗、没逐条核实，措辞要改中性 |
| [tanaer/WindowsClear](https://github.com/tanaer/WindowsClear) | MIT（Rust） | AppData 大目录搬到别的盘、原地建目录联接 | 只借思路 |
| [88lin/computer-repair-skill](https://github.com/88lin/computer-repair-skill) | AGPL-3.0 | 64 个维修流程文档 | 只参考知识 |
| [man612/Windows-Printer-Sharing-Fix](https://github.com/man612/Windows-Printer-Sharing-Fix) | MIT | 打印机共享「先诊断再修、每步快照」；RpcUseNamedPipeProtocol、RpcProtocols=7（我们已有）、RestrictDriverInstallationToAdministrators=0、AllowInsecureGuestAuth=1、LmCompatibilityLevel=1 | 后几项降低安全，只诊断提示 |
| MyComputerManager、Drive-Icon-Manager | GPL-3.0 | 「此电脑」和侧边栏删不掉的图标 | 我们已有同类，可对照 CLSID |
| DirectX 修复类（1wri/DirectXRepair 等） | 没有源码 | 转载闭源工具 | 不可用；缺 d3dx9 这类文件用官方的 DirectX 运行库（7.5） |

**微信、QQ 的目录和安全边界**（CleanMyWechat [main.py L1377-1447](https://github.com/blackboxo/CleanMyWechat/blob/master/main.py)、[selectVersion.py](https://github.com/blackboxo/CleanMyWechat/blob/master/utils/selectVersion.py)）：

- 微信 4.x：`文档\xwechat_files\<账号>\`（也可能在 `D:\xwechat_files`，要扫描）。**缓存**（可以默认勾选）：cache、temp、apm_record、business\InputTemp、business\emoticon\Temp、business\emoticon\Thumb、business\xweb。**聊天里的文件、图片、视频、收藏**（msg\file、msg\attach、msg\video、business\favorite）只能按「N 天以前」+ 进回收站 + 二次确认。
- 微信 3.x：`WeChat Files\<wxid>\FileStorage\{Cache,File,Image,Video,MsgAttach}`，只有 Cache 是缓存。位置看 `HKCU\Software\Tencent\WeChat` 的 FileSavePath（"MyDocument:" 表示在「文档」里）。
- 企业微信：`文档\WXWork\<id>\Cache\…`。
- QQ NT：`<数据目录>\<QQ号>\nt_qq\nt_data\{Pic,Ptt,Video,File,log}`（事实来自 NapCatQQ，只借事实；用户改过位置时怎么读，未核实）。
- **白名单以外的目录一律不碰，聊天数据库绝对不碰**；加一条测试：规则里不许出现对 `Tencent Files`、`xwechat_files` 根目录的递归删除。

---

## 六、国内高频问题：我们覆盖了多少

来源：微软问答中文区（2024–2026）、百度经验、知乎、CSDN、厂商 FAQ，加上微软官方文档。热度分档是定性估计：**A 档**是微软问答中文区有大量帖子、各平台教程很多、厂商 FAQ 也收录；**B 档**常见；**C 档**少见或有时效性。社区来源一律视为「未核实」，修法以官方文档为准。WebSearch 在美国，百度知道、贴吧几乎搜不到，各平台的浏览量拿不到。

「覆盖」一栏：✅ 已有；◐ 部分（有检测没修复，或者只在别的症状里顺带）；✗ 没有。「计划」对应第八节的优先级。

### 6.1 A 档（29 项）

| # | 用户的说法 | 常见原因 | 能一键修的部分（能否撤销） | 覆盖 | 计划 |
|---|---|---|---|---|---|
| A1 | 「WiFi 图标不见了」「没有 WLAN 选项，只有以太网」 | WLAN AutoConfig（WlanSvc）被禁用；无线网卡被禁用、驱动丢了 | WlanSvc 改回自动并启动；启用被禁用的网卡（都能改回） | ✗（「上不了网」查得出网卡被禁用，没查服务） | **P0** |
| A2 | 「已连接，无 Internet」「右下角小地球，其实能上网」 | NCSI 探测失败：代理、加速器、虚拟网卡；教程改过探测地址或关了主动探测 | 探测设置改回默认值（策略 NoActiveProbe 只检测） | ◐（「网络通不通」能查出探测被改，没有修复） | **P0** |
| A3 | 「QQ 微信能上，网页打不开」 | 代理残留、DNS、Winsock/LSP 损坏 | 关代理 ✅；Winsock 重置撤销不了，放最后一级 | ◐ | P1 |
| A4 | 「WiFi 老掉线」「睡醒后连不上网」 | 网卡勾了「允许计算机关闭此设备以节约电源」 | 关掉网卡省电（逐块网卡快照，能改回） | ✗ | **P0** |
| A5 | 「0x80070035 找不到网络路径」「看不到别的电脑」「来宾访问被阻止」 | Win11 24H2 起默认要求 SMB 签名、专业版默认拒绝来宾登录（2024–26 暴增的主因）；网络是「公用」；共享服务停了 | 网络改成「专用」、打开网络发现和文件打印机共享、恢复服务（能改回）；来宾、关签名、SMB1 降低安全，只说明 | ✗ | **P0** |
| A6 | 「微信占了几十 G」 | 微信 3.x 在 `文档\WeChat Files`，4.x 在 `文档\xwechat_files` | 缓存按白名单清，聊天文件按天数、进回收站、二次确认 | ◐（「C 盘被什么占了」能看出大小） | P1 |
| A7 | 「任务栏卡死、点了没反应」 | 资源管理器卡住；美化软件、输入法冲突 | 重启资源管理器 ✅ | ◐（有小工具，没有症状） | P1 |
| A8 | 「开始菜单打不开，Win 键没反应」 | StartMenuExperienceHost 异常 | 重启进程、重置开始菜单应用包 | ✗ | P1 |
| A9 | 「音量、网络图标不见了」 | 系统图标开关被关；优化软件写了 HideSCAVolume 这类策略 | 策略只检测说明 | ✗ | P1 |
| A10 | 「屏幕突然变黑白」「颜色反了」 | 误按 Win+Ctrl+C 打开了颜色滤镜（[微软](https://support.microsoft.com/zh-cn/accessibility/windows/use-color-filters-in-windows)） | `HKCU\Software\Microsoft\ColorFiltering` Active=0（能改回） | ✗ | **P0** |
| A11 | 「亮度条不见了」「分辨率只有一个」「字糊」 | 没装显卡驱动（显示「Microsoft 基本显示适配器」） | 查出来给驱动指引；Win+Ctrl+Shift+B 重置显卡驱动 | ◐（「电脑配置」能认出基本显示适配器） | P1 |
| A12 | 「接显示器没信号」 | 投影模式是「仅电脑屏幕」；线、Type-C 口不带视频 | 打开「投影」面板（Win+P） | ✗ | P1 |
| A13 | 「鼠标一卡一卡」「单击变双击」 | USB 选择性暂停；接收器没电；微动老化 | 关 USB 选择性暂停（电源设置，能改回） | ✗ | **P0** |
| A14 | 「触摸板没反应」 | Fn 键关了；设置里的开关 | 打开触摸板设置页 | ✗ | P2 |
| A15 | 「点关机又重启」「关机后自己开机」 | 快速启动异常；设备唤醒；蓝屏被自动重启掩盖 | 关快速启动 ✅；唤醒定时器 ✅ | ◐ | P1 |
| A16 | 「睡眠后黑屏唤不醒」 | 显卡、芯片组驱动和现代待机不兼容 | 合盖改成休眠（能改回） | ✗ | P2 |
| A17 | 「一会儿不动就黑屏、锁屏」 | 关屏、睡眠时间太短；屏保「恢复时显示登录屏幕」 | 调长超时（能改回） | ✗ | P2 |
| A18 | 「怎么取消开机密码」「PIN 不可用」 | 开了「仅允许 Windows Hello 登录」；PIN 数据损坏 | 降低安全，只给指引 | ✗ | P2（只指引） |
| A19 | 「需要安装 .NET Framework 3.5」「0x800f0950 / 0x800F081F」 | 要从 Windows 更新下载；「关更新」工具把更新源指到了无效地址；26H1 起不再是 Windows 功能 | DISM 在线安装；更新策略只检测 | ◐（能打开「启用或关闭 Windows 功能」） | P1 |
| A20 | 「软件界面乱码」「解压文件名乱码」 | 勾了「Beta 版：使用 Unicode UTF-8」（ACP=65001），老的 GBK 程序全乱码 | `HKLM\…\Control\Nls\CodePage` 的 ACP/OEMCP/MACCP 改回 936/936/10008（系统区域是中文时；要重启，能改回） | ✗ | **P0** |
| A21 | 「无法识别的 USB 设备」「代码 43」 | 供电、接口、线；选择性暂停 | 关选择性暂停；重新扫描硬件 | ◐（「U 盘插上没反应」「设备和驱动」） | P1 |
| A22 | 「手机连电脑只充电」 | 手机选了「仅充电」 | 只能给指引 | ✗ | P2（只指引） |
| A23 | 「不满足 Win11 要求」「Win10 停止支持怎么办」 | BIOS 没开 TPM、安全启动；CPU 不在列表 | 只读 | ✅ | — |
| A24 | 「开机要输 BitLocker 恢复密钥」 | 新机默认设备加密；刷 BIOS、换硬件会触发 | 预防：导出密钥 | ✅ | — |
| A25 | 「Windows 许可证即将过期」 | KMS 到期 | **不做激活**，只解释 | ✅（「电脑配置」显示激活状态） | — |
| A26 | 「开机黑屏只有鼠标」「进系统没桌面」 | 资源管理器没启动；`Winlogon` 的 Shell、Userinit 被病毒或美化软件改了（[微软：登录后黑屏](https://learn.microsoft.com/zh-cn/troubleshoot/windows-client/shell-experience/scenario-guide-black-screen-after-sign-in)） | Shell=explorer.exe、Userinit 改回默认（能改回） | ✗ | **P0** |
| A27 | 「自动修复 你的电脑未正确启动」 | 强制关机后文件系统损坏；更新、驱动冲突 | 进不去系统；预防是保持恢复环境可用 | ✅（WinRE 检测和开启） | — |
| A28 | 「前面板耳机没声音」 | Realtek 前面板插孔检测 | 只能给指引 | ◐ | P2 |
| A29 | 「蓝牙耳机一开麦音质变差」 | 切到了免提（HFP）模式 | 指引 | ✅（蓝牙症状的手动步骤） | — |

### 6.2 B 档（23 项）

| # | 用户的说法 | 原因和修法 | 覆盖 | 计划 |
|---|---|---|---|---|
| B1 | 「飞行模式关不掉、是灰的」 | RmSvc（无线电管理）或 WlanSvc 被停用 → 服务恢复默认 | ✗ | **P0**（和 A1 一起） |
| B2 | 「我们无法设置移动热点」 | Wi-Fi Direct 虚拟适配器被禁用、ICS 冲突、SharedAccess 被禁用 | ✗ | P2 |
| B3 | 「远程桌面连不上」「0x204」 | 家庭版不能被远程；没开远程；只用 PIN 登录 | ✗ | P2（只指引） |
| B4 | 「时间不对」 | W32Time 停用；主板电池没电 | ✅ | — |
| B5 | 「安全中心打不开」「由你的组织管理」 | 安全中心应用损坏（重置应用）；「关 Defender」工具留下的策略（只检测） | ◐ | P2 |
| B6 | 「微软商店打不开」「0x80131500」 | 代理、时间、缓存 → `wsreset.exe` | ◐（代理、时间） | P1 |
| B7 | 「图片打不开，文件系统错误 -2147219196」 | 照片应用损坏 → 重置；或用照片查看器 | ◐（照片查看器） | P2 |
| B8 | 「Edge 打不开、闪退」 | 设置 → 应用 → Edge → 修改 → 修复 | ✗ | P2 |
| B9 | 「此应用无法在你的电脑上运行」 | 32/64 位、ARM 不匹配；文件没下完 | ✗ | P2 |
| B10 | 「桌面图标变白」 | 图标缓存 | ✅ | — |
| B11 | 「右键『管理』打不开」「该文件没有与之关联的应用」 | 照教程去小箭头时删了 `HKCR\lnkfile\IsShortcut` → 补回（lnkfile、piffile） | ✗ | **P0** |
| B12 | 「所有 exe 都打不开」 | .exe / exefile 关联被病毒改了 → 恢复关联 | ✗ | **P0** |
| B13 | 「此电脑打不开、文件夹一开就卡」 | 失效的映射网络盘、预览处理程序、右键扩展 | ◐（右键菜单管理） | P2 |
| B14 | 「C:\ 上的回收站已损坏」 | 删掉 `$Recycle.Bin` 重建（会清空回收站，先提示）（[微软](https://learn.microsoft.com/zh-cn/troubleshoot/windows-client/shell-experience/recycle-bin-corrupted)） | ✗ | P1 |
| B15 | 「需要 TrustedInstaller 提供的权限」 | 旧系统目录 → 用存储清理处理 Windows.old | ◐ | P2 |
| B16 | 「D 盘不见了」 | NoDrives/NoViewOnDrive 策略（只检测说明） | ✅（U 盘症状里） | — |
| B17 | 「新装的固态硬盘不显示」 | 没初始化 → 磁盘管理里初始化（写盘，撤销不了）；**别乱改 SATA 模式**（会 0x7B 蓝屏） | ◐（能打开磁盘管理） | P2 |
| B18 | 「磁盘 100%」 | 机械盘 + 更新、索引 | ✅ | — |
| B19 | 「游戏没用独显、掉帧」 | 「图形」设置里是节能 GPU | ✗ | P2 |
| B20 | 「屏幕一闪一闪」 | 官方判断法：任务管理器也跟着闪是驱动问题，不闪是某个应用的问题（[微软](https://support.microsoft.com/zh-cn/windows/%E6%8E%92%E6%9F%A5-windows-%E4%B8%AD%E7%9A%84%E5%B1%8F%E5%B9%95%E9%97%AA%E7%83%81%E9%97%AE%E9%A2%98-47d5b0a7-89ea-1321-ec47-dc262675fc7b)） | ✗ | P1 |
| B21 | 「桌面文件不见了、图标上有绿勾」 | OneDrive 备份了桌面 | ✅（「桌面图标不见了」） | — |
| B22 | 「你已使用临时配置文件登录」 | 用户配置加载失败 | ✗ | P2（只指引） |
| B23 | 「开机要按 F1」「CPU Fan Error」 | 主板电池没电；风扇接错口 | ✗ | P2（只指引） |

### 6.3 C 档（9 项）

d3dx9_43.dll 丢失（✅ 指引 DirectX 运行库，数据待补全，见 7.5）；「应用程序的并行配置不正确」（14001，多为缺 VC++ 2005/2008，◐）；0xc000007b（◐，见 7.6）；「无法完成更新，正在撤销更改」「0x80070643」（✅ 已认出恢复环境更新失败）；屏幕倒过来了（Ctrl+Alt+方向键，✗，P2）；系统变英文（语言包，◐）；「Windows 已保护你的电脑」（SmartScreen，**不关**，只解释，P2）；风扇狂转（✅）；安全启动证书过期（✅ 只检测，官方说不影响开机）。

### 6.4 顺手核对的现有功能

调研里提到的几条「坑」，我们已经做对了：

- **显卡的 live dump（0x117、0x141）不会蓝屏**：「最近的蓝屏」只取系统日志里 WER-SystemErrorReporting 1001 和 Kernel-Power 41，不会把可靠性监视器里的 LiveKernelEvent 算成蓝屏。
- **Win10 消费者延长支持（ESU）到 2027-10-12**：`system.os-support` 写的就是这个日期。
- **微信 4.x 的 `xwechat_files`**：「C 盘被什么占了」已经同时认 3.x 和 4.x 的目录。
- **KB5034441 被 KB5042320 取代、恢复分区要 250MB**：`update.history` 已经认得。
- **WMIC 已移除**：脚本里没有 wmic、Get-WmiObject，全部用 CIM。
- **还原点 24 小时限制**：我们前后数一次、跳过时如实说明，没有谎报「已创建」。

## 七、要补进数据的官方对照表

这一节是给数据文件用的原材料，主要来源是微软官方文档；社区来源单独标出。

### 7.1 Windows 更新错误码（40 个）

来源：[常见 Windows 更新错误](https://learn.microsoft.com/en-us/troubleshoot/windows-client/installing-updates-features-roles/common-windows-update-errors)（下称 W1）、[Windows 更新错误码参考](https://learn.microsoft.com/en-us/windows/deployment/update/windows-update-error-reference)（W2）、[升级错误的解决办法](https://learn.microsoft.com/en-us/troubleshoot/windows-client/setup-upgrade-and-drivers/windows-10-upgrade-resolution-procedures)（W3）、[解决更新问题（中文）](https://support.microsoft.com/zh-cn/windows/deployment/updates-lifecycle/troubleshoot-problems-updating-windows)（W4）、Win32 错误码表（[0-499](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-)、[1300-1699](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1300-1699-)、[12000-15999](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--12000-15999-)）。

| 代码 | 官方含义 | 小白说法 | 建议 |
|---|---|---|---|
| 0x80070002 / 0x80070003 | 找不到文件 / 路径 | 更新要用的文件找不到 | 重置更新组件 |
| 0x80070005 | 拒绝访问 | 没有权限写文件 | 暂停第三方杀软；DISM、SFC |
| 0x8007000D | 数据无效 | 下载的更新坏了 | 清缓存重新下载 |
| 0x8007000E | 内存不足 | 资源不够 | 关掉程序重试 |
| 0x80070020 | 共享冲突 | 有软件（杀软、备份）占着文件 | 干净启动后重试 |
| 0x80070057 / 0x80080005 | 参数错误 / 文件损坏、权限、配置 | 配置有问题 | SFC |
| 0x80070070 / 0xC190020E | 磁盘空间不足（升级要 20GB） | C 盘不够 | 清理 C 盘 |
| 0x80070422 | 服务被禁用 | 更新服务被关了 | 已有「恢复 Windows 更新服务」 |
| 0x80070490 | 找不到元素（驱动信息缺失、不兼容的驱动） | 驱动不兼容 | 先更新驱动 |
| 0x80070570 | 文件损坏（组件存储） | 系统文件坏了 | DISM /RestoreHealth，再 SFC |
| 0x800705B4 | 超时 | 装得太久 | 检查网络、暂停杀软重试 |
| 0x80070643 | 安装时发生严重错误；2024 年的恢复环境更新因为恢复分区不够 250MB 失败（[KB5034441](https://support.microsoft.com/en-us/topic/kb5034441-windows-recovery-environment-update-for-windows-10-version-21h2-and-22h2-january-9-2024-62c04204-aaa5-4fee-a02a-2fdea17075a8)） | 安装包出错 | 先看是哪个更新；恢复环境那个按官方步骤扩分区（风险高，只给指引） |
| 0x80070652 | 已有别的安装在进行 | 在排队 | 等一会儿或重启 |
| 0x80070BC9 | 需要重启（策略改了 TrustedInstaller、Windows Installer 的启动类型） | 服务被优化软件改了 | 恢复默认启动类型后重启 |
| 0x80073701 / 0x80073712 / 0x8007371B | 组件缺失 / 组件存储损坏 / 事务不完整 | 系统组件库坏了 | DISM、SFC；不行就「用 Windows 更新修复问题」 |
| 0x800706BE | 上一个累积更新没装好 | 上次没装好 | 重置更新组件；重装当前版本 |
| 0x800F081F | 找不到修复用的源文件 | 修复原料找不到 | DISM；.NET 3.5 见 7.6 |
| 0x800F0831 / 0x800F0825 | 组件存储损坏 / 无法卸载 | 组件库坏了 | DISM、SFC |
| 0x800F0821 / 0x800F0920 | 维护操作超时、被判定卡死 | 太慢了 | 关掉后台程序 |
| 0x800F0922 | 安装程序失败；中文页说常见是磁盘空间不足 | 装到一半失败 | 断开 VPN 或代理软件；腾出空间 |
| 0x80240017 | 不适用 | 这个更新不适合你的电脑 | 不用管 |
| 0x80240020 | 没有登录的用户 | 要有人登录 | 登录后重启 |
| 0x80240022 | 全部失败（常见是杀软挡了 SoftwareDistribution） | 被杀软拦了 | 暂停第三方杀软 |
| 0x80240034 | 下载失败 | 下载失败 | 检查代理；重置更新组件 |
| 0x8024402C | 服务器名解析不出来 | DNS 或代理不通 | 关代理；查 hosts |
| 0x8024401B / 0x80244018 | HTTP 407 代理要认证 / HTTP 403 | 被代理拦了 | 关代理 |
| 0x80244022 | HTTP 503 | 服务器忙 | 稍后再试 |
| 0x80072EE2 / 0x80072EFD / 0x80072EFE / 0x80D02002 | 连接超时或被异常关闭 | 连不上更新服务器 | 检查网络、代理、防火墙 |
| 0x80072F8F | 安全连接失败（TLS） | 时间不对最常见 | 同步系统时间 |
| 0x80242006 | 更新信息无效 | 更新信息乱了 | 重置更新组件（已有） |
| 0x80242014 | 要重启才能完成 | 要重启 | 重启 |
| 0x80246017 | 不是管理员 | 权限不够 | 用管理员账户 |
| 0x8024A10A | 服务闲置太久关了 | 放太久了 | 保持电脑活动后重试 |
| 0xC1900101（-0x20017、0x30018、0x4000D 等） | 升级回滚，多数是驱动不兼容 | 升级时驱动不兼容，退回去了 | 拔外设；更新驱动、BIOS；卸第三方杀软 |
| 0xC1900107 | 上次安装的清理还没完成 | 上次的残留 | 重启；磁盘清理 |
| 0xC1900200 / 0xC1900202 / 0xC1900208 | 不满足最低要求 / 兼容性扫描没通过 | 配置不够或有软件不兼容 | 看「能不能升级 Win11」；按兼容报告卸载软件 |

### 7.2 蓝屏终止代码（30 个）

来源：[Bug check code reference](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-code-reference2) 各代码的详情页；官方统计约 70% 的蓝屏由第三方驱动引起、10% 由硬件引起（[停止错误排查](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/stop-code-error-troubleshooting)）。

| 代码 | 名称 | 官方说明要点 → 常见原因 → 建议 |
|---|---|---|
| 0xA / 0xD1 | IRQL_NOT_LESS_OR_EQUAL / DRIVER_IRQL_NOT_LESS_OR_EQUAL | 驱动在过高的 IRQL 访问分页内存 → 新装的驱动、服务、杀软 → 回退驱动、卸载新软件 |
| 0x1E | KMODE_EXCEPTION_NOT_HANDLED | 内核异常没被捕获 → 驱动、BIOS 不兼容 → 禁用蓝屏上提到的驱动、更新 BIOS |
| 0x3B | SYSTEM_SERVICE_EXCEPTION | 从用户态转到内核时出异常 → 更新驱动、SFC |
| 0x50 | PAGE_FAULT_IN_NONPAGED_AREA | 引用了无效的系统内存 → 驱动、杀软、NTFS 损坏、内存（含显存）→ chkdsk、内存诊断 |
| 0x1A | MEMORY_MANAGEMENT | 严重内存管理错误，多数是硬件 → 内存诊断 |
| 0x7E | SYSTEM_THREAD_EXCEPTION_NOT_HANDLED | 系统线程异常 → 驱动、固件 |
| 0xEF | CRITICAL_PROCESS_DIED | 关键进程（csrss、wininit）终止 → 撤销最近的改动、SFC |
| 0x133 | DPC_WATCHDOG_VIOLATION | DPC 运行太久 → 驱动没按时完成工作 |
| 0x116 | VIDEO_TDR_FAILURE | 重置显卡驱动失败 → 显卡驱动、超频、散热、电源 |
| 0x117 | VIDEO_TDR_TIMEOUT_DETECTED | **只是 live dump，不会真蓝屏**，不要算进蓝屏 |
| 0x124 | WHEA_UNCORRECTABLE_ERROR | 致命硬件错误 → 过热、内存、CPU、超频 → 关超频和 XMP |
| 0x7B | INACCESSIBLE_BOOT_DEVICE | 启动时读不到系统分区 → 在 BIOS 里改了 AHCI/RAID、硬盘故障 → 撤销 BIOS 改动、启动修复 |
| 0xED | UNMOUNTABLE_BOOT_VOLUME | 挂载启动卷失败 → 启动修复、chkdsk /r |
| 0x24 | NTFS_FILE_SYSTEM | NTFS 损坏、坏道 → chkdsk；留出 10–15% 空间 |
| 0x7A | KERNEL_DATA_INPAGE_ERROR | 页面文件里的内核数据读不回来；参数 2：C000009C/C000016A 坏道、C000009D 线缆、C000009A 资源不足 |
| 0xC000021A | WINLOGON_FATAL_ERROR | 用户态子系统被破坏 → 第三方软件、系统文件不匹配 |
| 0x9F | DRIVER_POWER_STATE_FAILURE | 睡眠、唤醒时驱动电源请求超时 → 更新驱动和 BIOS |
| 0xA0 | INTERNAL_POWER_ERROR | 参数 1 = 0xB 表示**休眠文件太小** |
| 0x101 | CLOCK_WATCHDOG_TIMEOUT | 处理器没按时响应 → 关超频、更新 BIOS |
| 0x154 | UNEXPECTED_STORE_EXCEPTION | 内存存储组件意外异常 → 内存诊断、SFC |
| 0x19 / 0xC2 | BAD_POOL_HEADER / BAD_POOL_CALLER | 驱动越界写或错误的池请求 |
| 0x139 / 0x13A / 0x109 | KERNEL_SECURITY_CHECK_FAILURE / KERNEL_MODE_HEAP_CORRUPTION / CRITICAL_STRUCTURE_CORRUPTION | 驱动破坏了内核数据 → 卸载可疑的驱动级工具 |
| 0xFC | ATTEMPTED_EXECUTE_OF_NOEXECUTE_MEMORY | 驱动试图执行不可执行的内存 → 更新或卸载那个驱动 |
| 0xF4 | CRITICAL_OBJECT_TERMINATION | 关键进程意外退出 → 查硬盘健康、SFC（社区经验） |
| 0xEA | THREAD_STUCK_IN_DEVICE_DRIVER | 驱动线程死循环，多为显卡 |
| 0xFE | BUGCODE_USB_DRIVER | USB 驱动出错，参数 1 = 0x3 通常是硬件 → 拔掉 USB 设备和扩展坞 |

「最近的蓝屏」现在的代码表已经有 50 个代码（上面这些都在），还差**按参数细分**的解读：0xA0 参数 1 = 0xB 是休眠文件太小，0x7A 的参数 2 分得出坏道、线缆、资源不足，0xFE 参数 1 = 0x3 多半是硬件。

### 7.3 设备管理器错误代码（官方全表）

来源：[Device Manager error messages](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/device-manager-error-messages)、[支持页](https://support.microsoft.com/zh-cn/topic/error-codes-in-device-manager-in-windows-524e9e89-4dee-8883-0afa-6bca0456324e)。

| 码 | 中文解释 | 建议 |
|---|---|---|
| 1 | 没配置好 | 更新驱动 |
| 3 | 驱动坏了或内存不够 | 关掉程序；重装驱动 |
| 9 | 硬件报不出身份 | 联系厂商 |
| 10 | 启动失败（最常见；有 FailReasonString 时显示那段文字） | 更新或回退驱动 |
| 12 | 资源冲突 | 更新 BIOS |
| 14 | 要重启 | 重启 |
| 16 | 资源识别不全 | 手动配置 |
| 18 | 要重装驱动 | 卸载后扫描 |
| 19 | 注册表里的配置坏了 | 卸载重装；系统还原 |
| 21 | 正在移除 | 等待或重启 |
| 22 | **被禁用了** | 启用（可以一键） |
| 24 | 设备不在或驱动不全 | 重插或移除 |
| 28 | **没装驱动** | 装驱动 |
| 29 | BIOS 里关掉了 | 在 BIOS 启用 |
| 31 | 驱动加载失败 | 重装驱动 |
| 32 | **驱动服务被禁用**（优化软件常见） | 恢复启动类型（可以一键） |
| 33 | 资源转换失败 | 更新 BIOS |
| 34 | 需要手动设置 | 按说明书配置 |
| 35 | BIOS 信息不全 | 更新 BIOS |
| 36 | 中断配置不对 | 在 BIOS 改 IRQ |
| 37 | 驱动初始化失败 | 重装驱动 |
| 38 | 旧驱动还在内存里 | 重启 |
| 39 | 驱动坏了或丢了 | 重装驱动 |
| 40 | 服务注册表项坏了 | 重装驱动 |
| 41 | 驱动在，硬件找不到 | 检查硬件 |
| 42 | 重复设备 | 重启 |
| 43 | **设备自己报告故障**（USB、显卡常见） | 换口；重装驱动；检测硬件 |
| 44 | 被软件关掉了 | 重启 |
| 45 | 没连接（隐藏的旧设备） | 一般可以忽略 |
| 46 | 正在关机 | 不用处理 |
| 47 | 已经安全弹出但没拔 | 拔掉再插 |
| 48 | 驱动被系统拦截（已知有问题） | 向厂商要新驱动 |
| 49 | 注册表太大 | 删除不用的设备 |
| 50 | 属性应用失败 | 重装驱动 |
| 51 | 在等别的设备 | 先解决它依赖的设备 |
| 52 | **驱动签名验证失败** | 装有签名的官方驱动 |
| 53 | 被内核调试器占用 | 关闭内核调试 |
| 54 | 正在重置 | 重启 |
| 55 | 没登录时被 DMA 保护阻止 | 登录后再用 |
| 56 | 还在配置（网卡常见，多为暂时） | 等待；一直这样就网络重置 |
| 57 | 虚拟机直通失败 | 家用可以忽略 |

「设备和驱动」检测要和这张表对一遍，确保每个码都有中文说明。

### 7.4 打印机和共享的错误码

来源：[KB4599464](https://support.microsoft.com/en-us/topic/managing-deployment-of-printer-rpc-binding-changes-for-cve-2021-1678-kb4599464-12a69652-30b9-3d61-d9f7-7201623a8b25)（P1）、[KB5005652](https://support.microsoft.com/en-us/servicing/os/windows/2021/08/kb5005652-manage-new-point-and-print-default-driver-installation-behavior-cve-2021-34481)（P2）、[Win11 的 RPC 打印更新](https://learn.microsoft.com/en-us/troubleshoot/windows-client/printing/windows-11-rpc-connection-updates-for-print)（P3）、[0x6D9](https://learn.microsoft.com/en-us/troubleshoot/windows-server/networking/error-0x000006d9-when-you-share-printer)（P4）。

| 代码 | 含义 | 常见原因 | 修法 | 我们 |
|---|---|---|---|---|
| 0x0000011b | （Win32 错误码表里查不到） | 主机强制了 RpcAuthnLevelPrivacyEnabled=1，和客户端补丁不一致（对应关系来自社区） | 两边都装最新更新；网络打印机直接按 IP 添加；主机设 0 可兼容但官方「不推荐」（P1） | ✅ |
| 0x00000709 | ERROR_INVALID_PRINTER_NAME | 2021-10 更新后连接共享打印机；Win11 22H2+ 默认 RPC over TCP | RpcUseNamedPipeProtocol=1（P3） | ✅ |
| 0x00000bcb | ERROR_PRINTER_DRIVER_DOWNLOAD_NEEDED | 2021-08 起非管理员不能通过「指向并打印」装驱动 | 用管理员先装同型号驱动；RestrictDriverInstallationToAdministrators=0 降低安全，官方说没有等效缓解（P2），只说明 | ✗ → P1 |
| 0x0000007c / 0x000006e4 | ERROR_INVALID_LEVEL / RPC_S_CANNOT_SUPPORT | 和 0x709 同一批 | 同 0x709 | ◐ |
| 0x000006d9 | EPT_S_NOT_REGISTERED | 共享打印机时「无法保存打印机设置」，因为**防火墙服务被停了** | 官方：防火墙服务设为自动并启动（P4） | ✗ → P1（mpssvc 在我们的禁改名单里，只能检测、给按钮） |
| 0x000006ba | RPC_S_SERVER_UNAVAILABLE | 打印服务没运行；22H2+ 的 RPC over TCP 被防火墙挡了 | 启动打印服务（✅）；放行端口或改命名管道 | ◐ |
| 0x000003e3 | ERROR_OPERATION_ABORTED | 驱动残留、共享设置（社区） | 删掉旧打印机和驱动包后重装（社区） | ✗ |
| 0x00000040 | ERROR_NETNAME_DELETED | Workstation/Server 服务（社区） | 启动这两个服务；改用 IP | ✗ |
| 0x80070035 / 0x80004005 | 找不到网络路径 / 未指定的错误 | 见 A5 | 见 A5 | ✗ → P0 |

### 7.5 缺哪个 DLL 装哪个官方运行库

| DLL 名 | 属于 | 官方下载 |
|---|---|---|
| msvcr80、msvcp80、mfc80(u) | VC++ 2005 SP1 | [微软下载 26347](https://www.microsoft.com/download/details.aspx?id=26347) |
| msvcr90、msvcp90、mfc90(u) | VC++ 2008 SP1 | [x86](https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x86.exe) / [x64](https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x64.exe) |
| msvcr100、msvcp100、mfc100(u)、vcomp100 | VC++ 2010 SP1 | [x86](https://download.microsoft.com/download/1/6/5/165255E7-1014-4D0A-B094-B6A430A6BFFC/vcredist_x86.exe) / [x64](https://download.microsoft.com/download/1/6/5/165255E7-1014-4D0A-B094-B6A430A6BFFC/vcredist_x64.exe) |
| msvcr110、msvcp110、mfc110(u) | VC++ 2012 Update 4 | [微软下载 30679](https://www.microsoft.com/download/details.aspx?id=30679) |
| msvcr120、msvcp120、mfc120(u) | VC++ 2013（12.0.40664） | [x86](https://aka.ms/highdpimfc2013x86enu) / [x64](https://aka.ms/highdpimfc2013x64enu) |
| vcruntime140(_1)、msvcp140(_1、_2、_atomic_wait、_codecvt_ids)、concrt140、vccorlib140、mfc140(u)、vcomp140 | VC++ v14（2015–2026 共用一套）；**32 位软件要装 x86 版** | 官方固定链接 [aka.ms/vc14/vc_redist.x64.exe](https://aka.ms/vc14/vc_redist.x64.exe)（x86、arm64 同理）（[最新受支持的 VC++](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)） |
| msvcp140d、vcruntime140d、ucrtbased（以 d 结尾） | 调试版，**官方禁止再分发** | 没有官方下载，找软件作者要正式版 |
| ucrtbase.dll、api-ms-win-crt-* | 通用 C 运行库，Win10 起系统自带 | Win10 以上缺它说明系统文件坏了 → SFC、DISM |
| api-ms-win-core-* 等 | API 集的「契约名」，**不是真实文件**（[API sets](https://learn.microsoft.com/en-us/windows/win32/apiindex/windows-apisets)） | 说明软件需要更新版本的 Windows；**不要去下载** |
| d3dx9_24–43、d3dx10_33–43、d3dx11_42/43、d3dcompiler_33–43、xinput1_1–1_3、xaudio2_0–2_7、x3daudio、xactengine | DirectX 最终用户运行时（2010 年 6 月版） | [微软下载 8109](https://www.microsoft.com/en-us/download/details.aspx?id=8109) |
| d3d9、d3d11、dxgi、d3dcompiler_47、xinput1_4、xaudio2_8/2_9 | Windows 自带 | 缺了去查显卡驱动或系统文件 |
| 「需要 .NET Framework 3.5 / 4.x」 | .NET Framework | 3.5 见 7.6；[4.8.1](https://dotnet.microsoft.com/download/dotnet-framework/net481) |
| hostfxr.dll、「must install or update .NET」 | .NET 5+ 桌面运行时 | [dotnet.microsoft.com](https://dotnet.microsoft.com/download/dotnet) |
| Microsoft.Xna.Framework*.dll | XNA 4.0 Refresh | [微软下载 27598](https://www.microsoft.com/en-us/download/details.aspx?id=27598) |
| OpenAL32.dll、wrap_oal.dll | OpenAL | [openal.org](https://www.openal.org/downloads/) |
| PhysXLoader、PhysXCore、PhysXDevice | NVIDIA PhysX | NVIDIA 官网 |
| mscomctl.ocx、comdlg32.ocx、msflxgrd.ocx | VB6 扩展控件（Windows 不自带） | [微软 VB6 支持说明](https://learn.microsoft.com/en-us/previous-versions/visualstudio/visual-basic-6/visual-basic-6-support-policy) |

我们的「软件打不开，提示缺少 dll」按文件名给出官方运行库，要和这张表对一遍，补上 VC++ 2005–2013 各版本的精确链接、调试版和 api-ms-win-core 的说明。**我们只自动安装 VC++ v14**（DOWNLOAD_PREFIXES 名单里的微软地址）；其他版本只给官方链接，由用户自己下载。

### 7.6 三个专题

**.NET Framework 3.5 装不上**（[安装错误](https://learn.microsoft.com/en-us/troubleshoot/windows-client/application-management/dotnet-framework-35-installation-error)、[Win10](https://learn.microsoft.com/en-us/dotnet/framework/install/dotnet-35-windows)、[Win11 26H1 起](https://learn.microsoft.com/en-us/dotnet/framework/install/dotnet-35-windows-11)、[部署错误](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/net-framework-35-deployment-errors-and-resolution-steps)）

- 0x800F0906：下载不到文件（网络、代理、WSUS）；0x800F081F：源路径里没有要的文件；0x800F0907：策略设成了「从不从 Windows 更新下载」；0x800F0922：源目录权限；0x800F0950、0x800F0954 官方没单列（社区：同 0906、0907，重点查 WSUS 残留）。
- 通用修法：「启用或关闭 Windows 功能」里勾选（25H2 及以前）；`Dism /online /enable-feature /featurename:NetFx3 /All`，离线时加 `/Source:<ISO>\sources\sxs /LimitAccess`（源必须是同一版本的 Windows）。
- 「关更新」工具留下的 `HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate` 下的 WSUS 设置会让下载失败——**这个键在我们的禁改名单里（不通过策略限制更新），只检测、说明**。
- **从 Win11 26H1（build 28000）起 .NET 3.5 不再是 Windows 功能**，只能用独立安装包，而且只对应那个 Windows 版本。

**0xc000007b**（[微软博客](https://learn.microsoft.com/en-us/archive/blogs/dsvc/diagnosing-status_invalid_image_format-c000007b-errors)）：STATUS_INVALID_IMAGE_FORMAT。最常见是**架构不匹配**（64 位程序加载了 32 位 DLL），或者文件损坏、根本不是程序。修法：x86、x64 两套运行库都装；重装软件；不要从 DLL 下载站拿单个文件。相关码：193「不是有效的 Win32 应用程序」、216 ERROR_EXE_MACHINE_TYPE_MISMATCH、14001「并行配置不正确」。

**「已连接，无 Internet」**（[NCSI 概述](https://learn.microsoft.com/en-us/windows-server/networking/ncsi/ncsi-overview)、[排查](https://learn.microsoft.com/en-us/troubleshoot/windows-server/networking/troubleshoot-ncsi-guidance)、[FAQ](https://learn.microsoft.com/en-us/windows-server/networking/ncsi/ncsi-frequently-asked-questions)）

- 工作方式：主动 HTTP 探测 `http://www.msftconnecttest.com/connecttest.txt`，期望内容 "Microsoft Connect Test"；DNS 探测 `dns.msftncsi.com` 期望解析为 131.107.255.255。Win11 由网络列表服务（netprofm）做探测。
- **默认值**在 `HKLM\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet`：EnableActiveProbing=1、ActiveWebProbeHost=www.msftconnecttest.com、ActiveWebProbePath=connecttest.txt、ActiveWebProbeContent=Microsoft Connect Test、ActiveDnsProbeHost=dns.msftncsi.com、ActiveDnsProbeContent=131.107.255.255（IPv6 各有一套对应的值）。
- **官方明确说不要用关闭主动探测来「解决」问题**；国内教程常把探测地址改成 msftncsi.com，那是 1607 之前的旧地址。所以一键修复应该是「恢复默认值」。策略 `HKLM\SOFTWARE\Policies\Microsoft\Windows\NetworkConnectivityStatusIndicator` 的 NoActiveProbe 只检测。
- 官方排查顺序：浏览器打开探测地址；检查代理、PAC（HTTP 403 通常是代理拦截）；看 Microsoft-Windows-NCSI/Operational 日志。探测服务器从 2023-06 起由 Akamai 托管，不要按 IP 放行。

---

## 八、缺口总表和开发计划

排序依据：①国内用户遇到的频率（第六节的档位）；②能不能一键修、能不能撤销；③风险和计划书第五节的约束；④有没有现成可复用的数据。**「只检测」**表示按规则只查出来、说清楚怎么办，不替用户改（策略、UAC、防火墙、Defender、IFEO、更新策略都属于这一类）。

### 8.1 P0：接下来直接做（价值高、风险低、能撤销、不需要新依赖）

| # | 做什么 | 形式 | 改哪里（能否撤销） | 来源 |
|---|---|---|---|---|
| 1 | **WiFi 不见了、飞行模式关不掉** | 新症状 + 检测 + 修复 | WlanSvc、RmSvc 恢复默认启动类型（服务原语，能撤销）；被禁用的无线网卡重新启用（脚本，撤销时再禁用） | 第六节 A1、B1；4.6 |
| 2 | **显示「无 Internet」其实能上网** | 给已有的「网络通不通」加修复 | NlaSvc\Parameters\Internet 的探测设置改回默认值（注册表原语，能撤销）；策略 NoActiveProbe 只检测 | A2、7.6 |
| 3 | **共享文件夹打不开、0x80070035** | 新症状 + 检测 + 修复 | 网络类型改为「专用」、打开「网络发现」「文件和打印机共享」的防火墙规则组（脚本，记下原状态，能改回）；LanmanServer、LanmanWorkstation、FDResPub、fdPHost 恢复启动类型（服务原语）；来宾登录、SMB 签名、SMB1 只检测、说明 | A5、7.4；[SMB 来宾登录](https://learn.microsoft.com/en-us/windows-server/storage/file-server/enable-insecure-guest-logons-smb2-and-smb3)、[SMB 签名](https://learn.microsoft.com/en-us/windows-server/storage/file-server/smb-signing)、[24H2 访问 NAS](https://techcommunity.microsoft.com/blog/filecab/accessing-a-third-party-nas-with-smb-in-windows-11-24h2-may-fail/4154300) |
| 4 | **屏幕突然变黑白** | 检测 + 修复 | `HKCU\Software\Microsoft\ColorFiltering` Active=0（能撤销） | A10 |
| 5 | **软件界面、解压文件名乱码** | 检测 + 修复 | 系统区域是中文、ACP=65001（UTF-8 Beta）时，Nls\CodePage 的 ACP/OEMCP/MACCP 改回 936/936/10008（要重启，能撤销） | A20 |
| 6 | **开机黑屏只有鼠标** | 检测 + 修复 | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon` 的 Shell、Userinit 改回默认（能撤销）；进不去桌面时的手动步骤 | A26 |
| 7 | **快捷方式、exe 打不开，右键「管理」打不开** | 检测 + 修复 | 补回 lnkfile、piffile 的 IsShortcut；.exe、exefile 的关联改回默认（能撤销） | B11、B12 |
| 8 | **鼠标一卡一卡、USB 设备老断开** | 检测 + 修复 | 当前电源计划的「USB 选择性暂停」关掉（电源设置，复用电源计划的共享代码块，能撤销） | A13、A21 |
| 9 | **WiFi、网线隔一会儿就断** | 检测 + 修复 | 已连接物理网卡的「允许计算机关闭此设备以节约电源」关掉（逐块网卡记录原值，能撤销） | A4；Sophia L5225 |
| 10 | **默认打印机老被换** | 打印机症状加修复 | `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Windows` LegacyDefaultPrinterMode=1（能撤销） | Sophia L4416 |
| 11 | **蓝屏一闪就重启，看不清代码** | 蓝屏症状加修复 | `CrashControl` AutoReboot=0、DisplayParameters=1（能撤销） | Disassembler0 L1781、winutil L1263 |
| 12 | **修复前的还原点每次都建** | 引擎改进 | 建还原点前临时把 SystemRestorePointCreationFrequency 设为 0，做完改回原值（微软给开发者的官方开关） | 4.7 |
| 13 | **数据补全** | 改数据 | 更新错误码 40 个（7.1）进「最近的更新」的翻译表；设备管理器错误码全表（7.3）；蓝屏按参数细分（7.2）；DLL → 官方运行库（7.5）；打印机错误码（7.4）进症状关键词和指引 | 第七节 |

**进展（2026-09-28）**：P0 的 13 项都已完成，和上表不同的地方：

- 第 3 项：防火墙规则组只检测、给按钮打开「高级共享设置」和「高级安全 Windows Defender 防火墙」，不替用户改（防火墙在禁改名单里）；SMB 来宾登录、签名同样只检测。
- 第 7 项：只做了 lnkfile 的 IsShortcut；exe 关联坏了时小药箱自己也打不开，只在指引里说明。
- 第 11 项：只改 AutoReboot；DisplayParameters 微软没有文档，没有加。
- 第 13 项：「最近的更新」认得 73 个代码、分 10 类，每个代码还有一句「这个代码的意思是……」（新加的检测数据字段 `fact_labels`，见 architecture.md 第 3 节）；0x800F0922 单独一类，「不适用」「被取消」这类不再算失败。设备管理器的 28 个出错代码各有一句说明。蓝屏按参数细分的除了 0x7A、0xA0、0xFE，还有 0x77；0xFE 的「从选择性暂停醒来超时」指向第 8 项的修复。
- 顺手加了两个小工具入口：「网络连接（网卡）」（control.exe netconnections）、「恢复（重装当前版本的 Windows）」（ms-settings:recovery）。

### 8.2 P1：接着做

| 做什么 | 形式 | 要点 | 来源 |
|---|---|---|---|
| 微信、QQ 占满 C 盘 | 「C 盘满了」加清理 | 缓存按白名单默认勾选；聊天里的文件按「N 天以前」+ 进回收站 + 二次确认；聊天数据库绝不碰；加测试禁止对 `Tencent Files`、`xwechat_files` 根目录递归删除 | 5.13、A6 |
| 被禁用的系统开关 | 新检测（只检测） | DisableTaskMgr、DisableRegistryTools、DisableCMD、NoFolderOptions、NoRun、NoControlPanel、NoViewContextMenu、NoClose、DisallowRun；IFEO 劫持任务管理器、注册表编辑器这类程序 | 4.1、4.2、4.3 |
| 开关机记录 | 新小工具（只读） | 41、42、1、1074、6005、6006、6008，按 41 的字段分蓝屏、长按电源键、断电 | 4.6 |
| 微软官方诊断入口 | 引擎扩展 + 各症状按钮 | 10 个 Get Help 名字进引擎名单；Win10 和 Win11 22H2 以前用 msdt 并读返回码；打不开时回到我们自己的检查 | 4.4 |
| WinHTTP 代理残留 | 检测 + 修复 | 记下 `netsh winhttp show proxy` 的原值，reset；撤销时设回原值；接到「更新失败」「商店打不开」 | 4.1 |
| Winsock 重置 | 修复（最后一级） | 要重启、撤销不了，确认框写清楚；先 `netsh winsock show catalog` 看有没有第三方 LSP | 4.1、4.2 |
| 任务栏卡死、开始菜单打不开、右下角图标不见 | 新症状 | 重启资源管理器（已有）、重置开始菜单应用包；HideSCA* 这类策略只检测 | A7–A9 |
| 屏幕显示不对 | 新症状 | 基本显示适配器（缺显卡驱动）；投影面板 Win+P；闪屏判断法；Win+Ctrl+Shift+B | A11、A12、B20 |
| .NET 3.5 装不上 | 新小工具 | DISM 在线安装；26H1 起提示独立安装包；更新策略只检测 | A19、7.6 |
| 应用商店打不开 | 小工具 | `wsreset.exe`；代理、时间已有检测 | B6 |
| 回收站已损坏 | 小工具 | 清空重建，**先提示会清空回收站** | B14 |
| 硬盘健康细则 | 改检测 | 机械盘 05、C5、C6 原始值 ≥1 报警告；NVMe 严重警告、备用空间；中文属性名 | 5.9 |
| 图片转文字 | 工具箱 | Windows.Media.Ocr；缺中文识别包时引导安装 | 4.5 |
| 键位重映射 | 工具箱 | Scancode Map，只做键对键；一键恢复 | 4.5 |
| 半夜被自动维护叫醒 | 「自己开机」加修复 | `…\Schedule\Maintenance` WakeUp=0（能撤销） | 5.5 |
| 计划任务里的可疑任务 | 小工具（只读 + 禁用能恢复） | 非微软、隐藏的计划任务，常是弹窗、半夜开机的元凶 | 4.6 |
| 常用设置批量 | 常用设置 | 任务栏右键「结束任务」、F1 不弹帮助、「下载」不分组、重启后不自动打开软件、搜索框不显示热点、滚动条常显、开机 NumLock、微软拼音默认英文/模糊音/关云候选、关贴靠布局、Alt+Tab 不列标签页、U 盘重复图标、盘符在前、Win11「此电脑」显示文件夹、建议类补全（338388、SystemPaneSuggestions、账户通知、备份提醒） | 5.2–5.7 |
| 修完做验证 | 界面 | 「没声音」修完链到喇叭测试，「麦克风」链到麦克风测试，「键盘」链到键盘测试 | 4.3 |
| **游戏、软件闪退诊断** | 新症状 + 小工具（只读） | 读应用程序日志 1000（崩溃）、1002（卡死）和可靠性记录，找出出错的模块，对应到运行库、显卡驱动或重装软件（联想「游戏闪退检测」78 万浏览） | 3.3、3.6、4.6 |
| **卸载最近的更新** | 小工具 | 列出最近装的更新和装完以后出的问题；打开「更新历史 → 卸载更新」；只卸用户选中的那一个 | 3.6（联想「补丁卸载」24 万） |
| **一键创建还原点** | 小工具 | 复用 `restore-point.ps1`（加上 24 小时开关）；系统保护没开时说明怎么开 | 3.6（6.6 万） |
| **暂停更新、使用时段、重启前提醒** | 更新症状里的入口 | 不「关」更新；打开系统自带的「暂停更新」；RestartNotificationsAllowed2、SmartActiveHoursState（5.4）能撤销 | 3.6（「关自动更新」合计 600 万浏览） |
| **恢复默认文件夹路径** | 检测 + 修复 | 「桌面」「文档」「下载」的 User Shell Folders 指向不存在的位置时改回默认（能撤销，不搬文件） | 3.6（6.4 万） |
| **键盘错误代码 19** | 「键盘没反应」加检测 + 修复 | 键盘类 `{4D36E96B-E325-11CE-BFC1-08002BE10318}` 的 UpperFilters 里除了 kbdclass 还有已经卸载的软件的驱动（先备份，能撤销） | 3.6、4.2 |
| **DirectPlay、老游戏组件** | 「缺 dll」加小工具 | `DISM /Online /Enable-Feature /FeatureName:DirectPlay /All`（Windows 自带的可选功能） | 3.1（1.7 万） |
| **VT（CPU 虚拟化）没开** | 检测（只读） | 安卓模拟器要用；Win32_Processor.VirtualizationFirmwareEnabled，告诉用户进 BIOS 开 | 3.1（1.7 万） |
| **远程求助** | 小工具 | 打开系统自带的「快速助手」，**醒目的反诈提示**（远程控制是诈骗高发手段，只让认识的人帮忙） | 3.2、3.6 |
| **「上不了网」补关键词和检查** | 数据 | 关键词：VPN 后上不了网、Ping 传输失败、多个默认网关、重定向次数过多、微信能上网浏览器打不开；加一项环境变量检查（PATH、SystemRoot 被改后 ping、ipconfig 失效） | 3.1、3.4 |
| **输入法简繁切换** | 「输入法」加检测 + 修复 | 微软拼音被切成了繁体（具体值待核实） | 3.1（3.5 万） |

**进展（2026-09-28）**：已完成下面这些，和上表不同的地方写在后面。

- 被禁用的系统开关：检测「被停用的系统工具」「程序被「劫持」（映像劫持）」和新症状「任务管理器、注册表打不开」，都只检测（策略、IFEO 在禁改名单里）。
- 半夜被自动维护叫醒、VT 没开、恢复默认文件夹路径、一键创建还原点（加了打开「系统保护」页的小工具）。
- .NET 3.5 装不上：小工具「安装 .NET Framework 3.5」（Dism.exe，错误代码按微软文档分类，「关更新」留下的更新服务器只检测、说明）；DirectPlay 做成同一段脚本的小工具；检测「老软件、老游戏要用的 Windows 组件」放进「缺少 dll」。
- 屏幕显示不对：新症状「屏幕模糊、分辨率不对、外接显示器没画面」，显卡驱动用已有的「设备和驱动」查（它本来就认得基本显示适配器）；分辨率是不是「推荐」值还没做（要读显示器的首选分辨率，考虑做成引擎内置检测）。
- 键盘错误代码 19：做成检测「键盘、鼠标、光驱这些设备的驱动登记」+ 修复「删掉失效的设备过滤驱动」，范围扩到鼠标、光驱、USB、摄像头；只删服务不在或者驱动文件不在的名字，键盘、鼠标缺的类驱动加回去，能撤销，执行前建还原点。
- WinHTTP 代理残留：检测「Windows 更新用的系统代理（WinHTTP）」+ 修复「关掉失效的 WinHTTP 代理」。和浏览器代理的修复一样只清标志位（按位撤销），只处理指向本机、没有程序在听的；别的机器上的代理只说明、不改（原计划的 `netsh winhttp reset proxy` 会清掉地址，撤销要整段写回，不如按位改）。放进「系统更新失败」。
- 远程求助：做成 action 小工具（引擎不用加「打开应用」的新类型）：登录用户装了快速助手就用 ms-quick-assist: 打开，没装的打开商店页面；醒目的反诈提示放在确认框里，结果里写清楚输代码、「允许」、随时断开。
- 应用商店打不开：小工具「重置应用商店缓存」（WSReset.exe，进程序名单）+ 新症状「应用商店打不开、下载不了」（查时间、两套代理、更新服务）。
- 开关机记录：小工具，一天一个表格；1074 只取程序文件名（按常见程序翻成「开始菜单」「Windows 更新」「关机命令」），41 按蓝屏、长按电源键、断电死机分开；重启靠「关机后 2 分钟内又开机」判断（1074 里的类型文字会被翻译，不用）。
- 常用设置批量（第一批 8 项）：任务栏右键「结束任务」、重启以后不自动打开软件、搜索框不显示热点、滚动条常显、关贴靠布局、Alt+Tab 不列标签页、盘符在前（写 HKCU，不用管理员）、开始菜单不提醒账户。值都对照过 winutil 或者两个以上的教程；F1、「下载」不分组、开机 NumLock、微软拼音的几项、U 盘重复图标、Win11「此电脑」显示文件夹还没做（要改 HKU\.DEFAULT、删系统的键，或者值还没核实）。
- 游戏、软件闪退诊断：小工具「最近闪退的软件」（应用程序日志 1000、1002，按出错模块的文件名和错误代码归成 11 类原因）+ 新症状「软件、游戏闪退、卡死」（查可靠性记录、VC++ 运行库、设备和驱动）。只输出文件名。
- 卸载最近的更新、暂停更新和使用时段：小工具「最近装的更新」（Windows 更新历史，按天，病毒库不列；读不了时列 KB 号）+「更新历史记录（卸载更新）」「使用时段」两个入口；检测「更新重启前的提醒和使用时段」+ 修复 RestartNotificationsAllowed2、SmartActiveHoursState（「设置」自己存开关的位置，不是组策略，能撤销）；新症状「装了更新以后出问题、老是自己重启更新」。常用设置加了「Windows 更新」一类。不替用户卸载更新（合并的累积更新用 wusa 卸不掉，要用户在「设置」里选）。
- 输入法简繁切换：检测「微软拼音是简体还是繁体」（`InputMethod\Settings\CHS` 的 `Output CharSet`，1 = 繁体；值对照了蓝点lilac 整理的微软拼音注册表说明）+ 修复「微软拼音改回简体」（注销再登录生效，能撤销），放进「输入法不见了」；结果里先教按 Ctrl + Shift + F 马上切回来、在设置里关掉这个快捷键。
- 任务栏卡死、开始菜单打不开、右下角图标不见：新症状 + 检测「被策略改动的任务栏和开始菜单」（HideSCA*、HideClock、NoTrayItemsDisplay、NoTrayContextMenu、NoSetTaskbar、锁定任务栏和开始菜单、去掉「所有应用」、DisableNotificationCenter 这 15 个值，值名对照了微软 ADMX_StartMenu、ADMX_Taskbar 文档和 ADMX 原文，只检测）+ 检测「开始菜单装好了没有、最近崩溃过没有」（照微软《Troubleshoot Start menu errors》：StartMenuExperienceHost 有没有给这个用户注册、文件还在不在，开始菜单、搜索、通知中心、资源管理器最近 7 天的 1000、1002）+ 小工具「重启开始菜单和搜索」（只结束 StartMenuExperienceHost、SearchHost、ShellExperienceHost 这几个，由资源管理器按用户身份拉起，比重启资源管理器轻）+「任务栏设置」入口。原计划的「重置开始菜单应用包」没有做成一键：微软说重新注册要在用户自己的、不是管理员的 PowerShell 里运行，小药箱以管理员身份运行时会装给管理员，所以只在结果里给懂哥写清楚命令。
- 「上不了网」补关键词和检查：关键词补了 Chrome 的几个错误代码、VPN 和加速器关掉以后上不了网、ping 传输失败、两个默认网关、重定向次数过多，指引里各给了一段；环境变量做成了单独的症状「命令提示符里提示「不是内部或外部命令」」+ 检测「系统命令找不找得到」（Path 是不是 REG_EXPAND_SZ、有没有 Windows 自带的文件夹，PATHEXT 有没有 .COM、.EXE、.BAT、.CMD，依据微软《Path Entry》《start》）+ 修复「修好系统的 Path 环境变量」（少了的 Windows 文件夹加回最前面，别的都留着，能撤销；改完借 .NET 删一个不存在的系统变量发出 WM_SETTINGCHANGE，新开的命令提示符不用注销就能用）+「环境变量」入口。SystemRoot 本身不在注册表的 Environment 里、用户改不了，没有查。
- 硬盘健康细则：机械硬盘（MediaType HDD）读 SMART 的 05、C5、C6 原始值（`MSStorageDriver_FailurePredictData` 的 VendorSpecific，按 PNPDeviceID 对上盘号），有不是 0 的报「警告」并说明是坏道，数字写进详情；固态硬盘不看这三项。NVMe 的严重警告、备用空间要 DeviceIoControl 读日志页（脚本里不能 Add-Type），没有做，先靠 Windows 自己报告的 HealthStatus。
- 计划任务里的可疑任务：先做了只读的小工具「计划任务里的第三方任务」（\Microsoft\ 以外的任务，和开机启动项共用「程序信息」共享块查程序、公司、签名；隐藏、会叫醒电脑、程序不在了、通过脚本或 rundll32 运行、在用户文件夹里又没签名、没签名的排前面）+「任务计划程序」入口，禁用由用户在任务计划程序里右键完成（能再启用）；小药箱自己一键禁用、能撤销的版本要像开机启动项那样在引擎里记修改日志，以后再做。「弹窗」「自己开机」两个症状加了按钮。「谁把电脑叫醒的」里的任务名顺手去掉了 SID。
- 修完做验证：先做了通用的一步——症状的手动步骤下面能放按钮（`links`），33 个症状按指引里提到的小工具补上了；再加了一种按钮 `test:<名字>`，跳到工具箱「屏幕、键盘、鼠标、声音测试」里的那一项（滚过去、突出显示）：「没声音」「麦克风、摄像头用不了」「键盘按了没反应」「鼠标卡顿、USB 老断开」「屏幕变黑白、颜色不对」的指引最后都加了「修好以后测一测」。
- 键位重映射：放在「常用设置」最下面（和右键菜单、「新建」菜单、资源管理器里的图标一起，是改系统设置、记修改日志的），不在工具箱。整张表一起保存到 `Scancode Map`（REG_BINARY，字节格式照微软《Keyboard and mouse class drivers》，单元测试里对了文档的两个例子），能选的键写在引擎里（名字和键盘测试一样用 `KeyboardEvent.code`），多媒体键（静音、音量、播放）只能当「变成」的键；常用改法一键加上（关掉 Win 键、关掉 Caps Lock、Caps Lock 改成左 Ctrl、关掉 Insert、关掉 F1）。别的改键工具设的、有名单外的键时只给「全部恢复」。修改日志里说成「Caps Lock（大写锁定） → 左 Ctrl」，撤销把原来的字节原样写回。保存以后给「测一测键盘」按钮。Windows 上的测试写真的注册表、另起 PowerShell 核对字节再撤销；重启以后是不是真的生效列进真机验证清单。
- 图片转文字：工具箱卡片（选图、拖进来、Ctrl + V 粘贴截图），用 Windows 自带的文字识别（Windows.Media.Ocr，按微软 PowerToys「文本提取器」文档的办法在 Windows PowerShell 5.1 里加载），不联网。优先简体中文；长截图分成每块最多 4000 像素、块间重叠 400 像素来认（10000 像素一块时 Windows 认字会丢字母、丢词，测试里发现的），一块认到分界线再往重叠里多认一点，前一块认过的同一行（位置相近、左右重叠）不再要（同一行在两块里认出来的位置会差几个像素，只按分界线一刀切会认两次或者丢掉；Windows 上的测试在交界附近放字核对不丢、不重复）；中文的字之间不加空格。没有中文识别的，给新的 action 小工具「安装中文文字识别」：能力名照微软《Language and region Features on Demand》的格式 `Language.OCR~~~zh-CN~0.0.1.0`（依赖同一语言的 `Language.Basic`），DISM 安装，下载不了的原因和 .NET 3.5 一样只检测。
- 微信、QQ 占满 C 盘：先只做了「C 盘被什么占了」里新旧两版微信文件夹的说明（升级到 4.x 时两边很多文件是硬链接的同一份，分别算的加起来比实际多；旧版留下的在新版微信「设置 → 存储空间」里清）。**一键清理暂缓**：微信 4.x 的 `cache` 按月放「消息、网页、表情、朋友圈、小程序」的缓存（wener.me 的笔记），3.x 的 `FileStorage\Cache` 有文章说里面是聊天图片，企业微信的 `Cache` 里按月放着文件、图片、语音（CleanMyWechat 的注释），光凭文章分不清删了会不会丢图；再加上硬链接，删一边不一定腾出地方。要先在真机上核对微信自己的「清理缓存」删的是哪些文件夹（列进真机验证清单），再照它做白名单。
- （P2 里的「报错弹窗截图识别」先做了一半）看报错截图：「按症状修」里粘贴报错窗口的截图，认出字以后按症状的关键词对上症状，错误代码单独列出来；顺手补了症状「设备用不了、设备管理器里有黄色感叹号」（包住已有的检测 hardware.device-problems，关键词是设备管理器的原话），和几条关键词（`ms-windows-store`、「USB 设备无法识别」）。20 条常见报错原话里 17 条对得上，剩下的「此应用无法在你的电脑上运行」「回收站已损坏」「文件已在另一个程序中打开」还没有对应的症状（「回收站已损坏」已经补上，见下一条；「文件已在另一个程序中打开」补了症状「删不掉、改不了名」，页面上放工具箱里已有的「文件删不掉：是谁占着」；「此应用无法在你的电脑上运行」补了同名症状和「看看这个程序能不能在这台电脑上运行」：照微软 PE 格式只读文件头，P2 里的「查 exe 架构」就此做了。20 条都对得上了）。错误码规则库（把代码直接翻成人话）以后再做。
- 微软官方诊断入口：「获取帮助」的 10 个自动疑难解答做成打开类小工具（open 新加了 `troubleshooter`，名字照微软《Windows troubleshooters》页面，引擎里有名单），接到没声音、蓝牙、麦克风和摄像头、上不了网、老断网、WiFi 不见了、打印机脱机和共享打印机、更新失败、软件闪退这些症状的按钮上；另加「设置 → 疑难解答」的入口（ms-settings:troubleshoot，微软 ms-settings 文档里有）。没有「获取帮助」应用时如实说明。和原计划不同：Win10 和 Win11 22H2 以前没有改用 msdt 读返回码——msdt 已经在退役，`ms-msdt:` 协议还是 Follina 的入口；这些系统上「获取帮助」打不开时，让用户到「设置 → 疑难解答」里用系统自带的老疑难解答（列进真机验证清单：Win10 上「获取帮助」认不认这些名字）。
- 临时配置文件：P2 里「只给指引的症状」先做了这一个，而且查得出来：检测「是不是用临时配置文件登录的」看登录用户的 Volatile Environment 里 USERPROFILE 是不是 TEMP、TEMP.xxx，另外记 ProfileList 里有没有 `<SID>.bak`、最近 30 天的事件 1511，只输出有没有和次数；新症状「提示『你已使用临时配置文件登录』」（还查 C 盘空间、硬盘读写错误），「桌面图标不见了」也加了这一步。改 ProfileList 的修法只写进指引、交给懂哥。
- 屏幕倒过来了：又一个只给指引的症状「屏幕倒过来了、横过来了」：先试 Intel 显卡的转屏快捷键（Ctrl + Alt + ↑），不行就在「显示方向」里改回「横向」（15 秒内要点「保留更改」），多显示器先选对屏幕，平板用快速设置里的「旋转锁定」；按钮打开「屏幕」设置。改方向要调显示器的模式（ChangeDisplaySettingsEx），做成一键修的收益不大，先不做。
- 开机要按 F1：只给指引的症状「开机要按 F1 才能进系统」（B23）：CMOS 报错按主板电池没电处理（查系统时间：电池没电时时间会跳回过去），CPU Fan Error 先看风扇转不转、再改插 CPU_FAN 或者在 BIOS 里调低报警，「Defaults Loaded」只出现一次的保存退出就好；写明 BIOS 里别动硬盘模式。开机画面拍照粘贴到「按症状修」也能对上（测试加了两条）。
- 手机只充电：只给指引的症状「手机连电脑只充电，传不了照片和文件」，查设备管理器（手机这类便携设备出问题时只计数、不列名字）；N 版 Windows 没装「媒体功能包」时认不出手机里的文件，按微软《Media Feature Pack list for Windows N editions》写进指引，没做检测（国内很少见 N 版）。
- 误删文件：先做了只给指引的症状「误删了文件，怎么找回来」（撤销、回收站、「以前的版本」、网盘回收站、聊天里重新下载，最后才是微软的 Windows File Recovery，命令照微软的例子写）。照「先找能复用的」找过的开源项目：kil0bit-kb/winfr-pro（MIT，Tauri + Rust，和小药箱同一套技术，给 winfr 做的图形界面：按文件系统选 /regular 或 /extensive、/a 免确认、winfr 管道输出是 UTF-16LE、进度按百分比和 Pass 1/2 算、结果在 Recovery_日期_时间 文件夹里、exFAT 上 /o:b 会让 winfr 崩溃）、6r6/Windows-File-Recovery-GUI（GPL-3.0，Go + GoVCL，只是简单的界面）；傲梅的 WinfrGUI 是免费软件、不开源，GitHub 上几个同名仓库是转载下载链接的，不用。接着做了页面上的「拼出恢复命令」（app/src/utils/fileRecovery.ts，按类型分组参考 winfr-pro，加了 WPS 格式、HEIC、AMR；NTFS 以外的盘只能 /extensive；不用 /o:b 和 /a）和小工具「打开微软的 Windows File Recovery」（和「远程求助」共用找商店应用的共享块 store-package：装了按应用清单里的 ID 从 shell:AppsFolder 打开，没装打开商店页面）。没照 winfr-pro 在小药箱里直接运行 winfr：winfr 在用户目录的应用别名里，以管理员身份从用户能写的位置运行程序有被冒名顶替的风险，而且 CI 上装不了它、没法验证输出格式；先让用户在它自己的窗口里粘贴命令、按 y 确认，真机验证以后再考虑。
- 常用设置第二批先做了「微软拼音默认是英文」（`Default Mode` = 1，蓝点lilac 的清单和无忧启动论坛的 PE 教程都是这个值）。微软拼音的「关云候选」不做：Win11 24H2 上微软拼音弹推广时会自己把 `Enable Cloud Candidate` 改回 1（绕过设置页和组策略），改了也留不住；「模糊音」要连 `Fuzzy Pair Setting` 的默认值一起核实，先不做。F1 不弹帮助：常用设置里的「键位重映射」已经能把 F1 设成不起作用（会连带其他程序里的 F1），只关 Windows 帮助的 HKCU TypeLib 办法还没核实。
- Winsock 检查和重置：检测「网络组件（Winsock）有没有被改坏」做成引擎内置检测（脚本里不能 Add-Type，读不了 Winsock 目录），用微软公开的 WSCEnumProtocols、WSCGetProviderPath 读 64 位和 32 位程序用的两份目录，查第三方 LSP（协议链长度不是 1）、文件已经不在的组件、有没有 TCP/IP；修复「重置 Winsock」就是 `netsh winsock reset`（微软说它不动名称空间提供程序，所以检测也只看协议目录），撤销不了、要重启，执行前建还原点。放进「上不了网」的第一步（查出问题也不停：LSP 坏了时后面几步会被连累）和体检。原计划先看 `netsh winsock show catalog`：它的输出按系统语言翻译过，不好解析，改用 API。TCP/IP 重置（`netsh int ip reset`）还没做。
- 回收站已损坏：做成新症状「提示『回收站已损坏』」+ 页面上的「清空并重建回收站」卡片（不是小工具：要选盘，小工具不带参数）。办法照微软《The Recycle Bin is corrupted》：删掉那个盘的 `$Recycle.Bin`，重启以后 Windows 重新建。先列出各个盘的回收站里有多少个文件、多大，确认框里写清楚这台电脑上所有账户的都会永久删除；只删这一个文件夹，链接只删链接本身。关键词是提示框的原话（中文、英文都有）；手动步骤先让用户点提示框里的「是」，一再提示才用卡片；一再损坏的多半是盘有问题，检查步骤查硬盘读写错误（含文件系统损坏）和硬盘健康。

### 8.3 P2：以后再评估

**软件卸载 + 残留清理**（照火绒强力卸载的流程：先调软件自带卸载 → 扫残留 → 处理占用进程 → 删不掉的重启后删；残留用 BCU 的打分规则加中英名称对照，默认只列把握最大的，删前备份、进回收站）；**报错弹窗截图识别**（系统自带 OCR + 错误码规则库，学联想 2026-02 的「异常报错弹窗处理」）；Office 修复（调用 Office 自带的修复）；读出品牌、型号，给官方驱动和保修页；本地 AI 智能体（「龙虾」）的检查和停止；全屏游戏时任务栏不隐藏；快捷键被占用（Alt+Z）；U 盘 FAT32 无损转 NTFS（不可逆，要明确提示）；误删文件引导用微软的 winfr；默认应用被改的提示；旧驱动包清理（pnputil，先导出备份，只删没绑定设备的旧版本）；按应用统计流量（读 SRUM）；外接显示器亮度（DDC/CI）；PATH 体检；触摸板、合盖动作、自动息屏、独显设置、移动热点、远程桌面、Edge 修复；JFIF 改回 JPG；Win11 CPU 表兜底；「找大文件」用 MFT 提速（要加 Rust 依赖，得先征得同意）；WMI 自检；性能计数器修复；光驱 UpperFilters；「此应用无法在你的电脑上运行」查 exe 架构；安全中心应用重置；屏幕倒过来；手机只充电、临时配置文件、F1 这类只给指引的症状。

## 九、明确不做

和[计划书](plan.md)第五节一致，调研里看到的这些做法一律不搬：

- **关 Defender、UAC、SmartScreen、防火墙，禁用更新**：winutil 的遥测项顺带改 Defender、Winaero 的禁 Defender/更新/UAC、Tweaking 的「UAC 恢复默认」（我们只检测）。
- **删策略、重置权限**：winutil 的「Windows 更新重置」删整棵 Policies 和 GroupPolicy 目录；Tweaking 的批量重置注册表和文件权限（计划书第五节第 28 条）。
- **危险的清理规则**：Winapp2 的 `[Tencent QQ *]`（会删光聊天记录）、WPS backup、*.bak；Dism++ 的「腾讯相关软件下载目录」（删整个 ProgramData\temp）、Windows 日志全删；Sophia 删 `C:\Recovery`；privacy.sexy 的 DisableResetbase=0、清事件日志、卷影副本、Prefetch；winutil 磁盘清理的 `/ResetBase`；BleachBit 的内存转储（删 Minidump 之前要先做蓝屏分析）。
- **降低安全的「兼容」**：打开 SMB1、来宾登录、关 SMB 签名、RestrictDriverInstallationToAdministrators=0、LmCompatibilityLevel=1——只在检测结果里说明。
- **其他**：隐藏快捷方式小箭头（Blank.ico 缺失会变黑块）、关闭打开程序的安全警告、快速关机（WaitToKillAppTimeout，会丢文档）、hosts 屏蔽、删系统应用、BitLocker 解密、bcdedit（我们的 lint 本来就禁止）、激活。
- **不打包别人的程序**：Sysinternals 许可禁止再发布；NirSoft 容易被杀软误报；DirectX 修复工具这类转载的闭源程序不可用。

## 十、待核实

- 「获取帮助」疑难解答在离线、国内网络下能不能用；Win10 上 `msdt /id` 各包名是否还在。
- 微信 4.x 各子目录的真实含义（cache、temp 之外的 business 下各目录），QQ NT 数据目录被用户改过位置时怎么读。
- 中文 OCR 识别包：能力名的格式已经按微软《Language and region Features on Demand》核实（`Language.OCR~~~<语言>~0.0.1.0`，依赖同一语言的 Basic），也做了用 DISM 安装的小工具；还要在真机上核实英文版、精简版系统上装不装得上、要下载多少（列进真机验证清单）。
- NCSI 默认值在 Win10、Win11 各版本上是否一致（尤其 IPv6 那一套）。
- 「USB 选择性暂停」「网卡省电」关掉以后对笔记本续航的影响；PnPCapabilities 的取值含义。
- 系统区域改回 936 以后，Beta UTF-8 装过的软件有没有副作用。
- winutil 的「任务栏结束任务」从哪个版本起生效（文中写 22631）；电池百分比开关的生效版本。
- 0x800F0950、0x800F0954 的官方说明；Servicing 策略各值的含义。
