# 真机验证清单

CI 在 Windows Server 虚拟机上跑脚本和往返测试，能发现大部分问题，但下面这些只有在真实的家用电脑上才能确认。写代码时查不到官方依据、或者依赖具体硬件的地方，都记在这里。验证完一项，就在对应的 YAML 或脚本注释里写明结果，然后从这里删掉。

建议的测试机：Win11 家庭中文版（最新两个版本）、Win11 专业版、Win10 22H2 家庭版，至少一台笔记本、一台带机械硬盘的老台式机。

## 检测

| 检测 | 要确认什么 | 怎么验证 |
| --- | --- | --- |
| `system.winre-status` | `ReAgent.xml` 里 `InstallState` 的含义（1 = 开，0 = 关）来自社区工具，微软没有公开这个格式；文件不存在时现在显示「没查出来」 | 分别在 `reagentc /disable` 和 `reagentc /enable` 之后运行检测，对照 `reagentc /info` |
| `system.managed-device` | 排除内部注册项的名单来自社区工具 | 在家庭中文版、只登录了微软账户、只在 Office 里添加了工作账户的电脑上，都应判为「个人电脑」 |
| `security.secureboot-ca2023` | 传统 BIOS 电脑上 `Confirm-SecureBootUEFI` 抛出的具体异常 | 在一台传统 BIOS 启动的电脑上运行 |
| `hardware.system-disk-type` | Intel RST / VMD 的 RAID 模式下报的 BusType、MediaType、型号名是什么：现在 RAID 总线一律判「判断不了」（unsure），除非型号名里看得出 SSD / NVMe；eMMC 小本报的总线类型 | 在开了 VMD / RAID On 的新笔记本和 eMMC 小本上运行，对照任务管理器「性能」页的磁盘类型；如果 RAID 下能可靠拿到 SSD，就把 unsure 收窄 |
| `hardware.memory-size` | 板载内存、部分 BIOS 报的插槽数不准；虚拟机里插槽数是 0 | 在板载内存的笔记本上对照任务管理器 |
| `hardware.battery-health` | 接了 UPS 的台式机会不会把 UPS 当成电池；电池彻底坏了（系统显示「0% 可用」）时，电池报告里的满电容量是不是 0（现在按 0 判为「老化严重」） | 在接 UPS 的台式机上运行；找一台电池坏了的旧笔记本运行 |
| `boot.last-boot-duration` | 开着快速启动时，「关机再开机」会不会记录事件 100；现在按 MainPathBootTime（按下电源到出现桌面）判断快慢，要确认它是否包含停在登录界面、输入密码的时间（包含的话，设了密码的电脑会被误判为慢） | 开快速启动，关机再开机后运行；在登录界面停一分钟再输密码，对照 MainPathBootTime 和 UserLogonWaitDuration |
| `system.reliability-recent` | 精简系统可能关掉了可靠性数据收集，这时会误显示「正常」 | 在关闭了 RacTask 计划任务的电脑上运行 |
| `disk.error-events` | 现在按盘号（`\Device\HarddiskN`）和盘符排除 USB 盘和已经拔掉的盘，要确认：153 的插入字符串里确实有 `\Device\HarddiskN`；Ntfs 98 的「需要完整磁盘检查」是错误 / 警告级别、插入字符串里有盘符（「卷正常」那条是信息级别，不计数）；U 盘上的 Ntfs 55 能按盘符排除 | 拔一次正在读写的 U 盘后运行，看 `excluded_removable`；在事件查看器里对照一条真实的 153 和 98 |
| `disk.smart-health` | 查不到健康状态（Unknown）的内置盘现在判为「没法确定」（incomplete），不再算进「状态良好」：要确认 RAID / Intel RST 下系统盘是不是 Unknown；读卡器里的 SD 卡、eMMC 系统盘各报什么 BusType（SD / MMC 上只有系统盘算内置） | 在开了 RAID 的电脑、带读卡器的笔记本、eMMC 小本上运行 |
| `security.bitlocker-status` | 「等待激活」（已加密但没启用保护）现在靠 `GetKeyProtectors` 返回空来识别；「保护已暂停」有保护器，仍按「已开启」处理 | 在只用本地账户装好的 Win11 24H2 家庭版上运行，对照 `manage-bde -status` 里的「密钥保护器」；再在暂停了 BitLocker 的专业版上运行 |
| `network.proxy-dead` | 拨号、VPN 连接的代理：`Internet Settings\Connections\<连接名>` 的格式是否和局域网的一样；连着 PPPoE 时 WinINet 是否真的用这个连接的设置（微软 IE 团队博客是这么说的） | 在直接 PPPoE 拨号的电脑上，用 v2rayN 打开系统代理后强行结束它，运行检测，看是否报「拨号或 VPN 连接的代理失效」 |
| `system.os-support` | Win11 26H2（build 26300）正式发布后要把日期补进表里；国内能否在「Windows 更新」里完成免费 ESU 登记 | 26H2 发布后更新表格；在 Win10 22H2 家庭版上走一遍 ESU 登记 |

## 修复

| 修复 | 要确认什么 | 怎么验证 |
| --- | --- | --- |
| `network.proxy-off` | 现在只关「指向本机、又没有程序在听」的手动代理，范围是 `ProxyEnable`、`DefaultConnectionSettings` 和每个拨号、VPN 连接的二进制值（按位清掉标志 0x02、计数器加 1，其余字节不动；这个格式是社区逆向出来的）；撤销也按位恢复。还要确认：改完是否立刻生效（脚本没法通知正在运行的程序，已打开的浏览器可能还在用旧设置）；重启电脑后是否保持关闭（没有被 `SavedLegacySettings` 或别的程序改回来）；PPPoE 连着时修复后能否上网；撤销后拨号连接的代理是否回到原样 | 用代理软件制造「代理残留」后执行修复：已打开的浏览器能否马上上网、重开浏览器后能否上网、「设置 → 网络 → 代理」里显示什么；重启电脑后再运行 `network.proxy-dead`，看 `legacy_proxy_enabled`、`blob_proxy_enabled`、`dialup_proxy`；在 PPPoE 拨号的电脑上再做一遍，并在修改日志里撤销一次 |
| `start.disable-web-search` | 微软文档列出的适用版本是专业版、企业版、教育版，社区测试说家庭版直接写注册表也有效；只重启资源管理器够不够；描述里写的两个副作用（「搜索权限」页显示「某些设置由你的组织管理」、「搜索要点」开关变灰）是否属实，撤销后是否消失 | 在家庭中文版上执行，注销再登录后在开始菜单搜索，打开「设置 → 隐私和安全性 → 搜索权限」看；再在修改日志里撤销 |
| `taskbar.align-left` | 是否不用重启资源管理器就立刻生效（现在保守地写了 `reboot: explorer`） | 执行后观察任务栏 |
| `explorer.classic-context-menu` | 写到登录用户的 `HKU\<SID>\Software\Classes`（它链接到 `HKU\<SID>_Classes`）是否生效；新版本 Win11 是否还支持 | 执行后重启资源管理器，右键桌面 |

## 引擎和外壳

| 项目 | 要确认什么 |
| --- | --- |
| 以另一个管理员账户提权 | 标准用户登录、用管理员账户的密码提权时，修改是否写到了登录用户身上（引擎通过控制台会话里 `explorer.exe` 的令牌找登录用户） |
| WebView2 检测 | Win11 自带的 WebView2 在注册表里是否都能查到；查不到时界面起不来的提示是否正确 |
| 还原点 | 系统还原被关闭、24 小时内已经建过还原点这两种情况下，提示文字是否准确 |
| 老电脑的启动速度 | 机械硬盘的老电脑上，PowerShell 冷启动加第一个检测要多久；体检的总时间 |
| 杀毒软件 | 火绒、360、腾讯电脑管家会不会拦截脚本宿主或者报毒 |
| 小屏幕和缩放 | 1366×768（100%）、1920×1080（125%、150%）的笔记本上，窗口是否完整显示在任务栏上方；最小高度 500 时侧栏和各页是否还能正常使用（CI 只在 1024×768、100% 缩放下测过） |
