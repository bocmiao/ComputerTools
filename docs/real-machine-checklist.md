# 真机验证清单

CI 在 Windows Server 虚拟机上跑脚本和往返测试，能发现大部分问题，但下面这些只有在真实的家用电脑上才能确认。写代码时查不到官方依据、或者依赖具体硬件的地方，都记在这里。验证完一项，就在对应的 YAML 或脚本注释里写明结果，然后从这里删掉。

建议的测试机：Win11 家庭中文版（最新两个版本）、Win11 专业版、Win10 22H2 家庭版，至少一台笔记本、一台带机械硬盘的老台式机。

## 检测

| 检测 | 要确认什么 | 怎么验证 |
| --- | --- | --- |
| `system.winre-status` | `ReAgent.xml` 里 `InstallState` 的含义（1 = 开，0 = 关）来自社区工具，微软没有公开这个格式；文件不存在时现在显示「没查出来」 | 分别在 `reagentc /disable` 和 `reagentc /enable` 之后运行检测，对照 `reagentc /info` |
| `system.managed-device` | 排除内部注册项的名单来自社区工具 | 在家庭中文版、只登录了微软账户、只在 Office 里添加了工作账户的电脑上，都应判为「个人电脑」 |
| `security.secureboot-ca2023` | 传统 BIOS 电脑上 `Confirm-SecureBootUEFI` 抛出的具体异常 | 在一台传统 BIOS 启动的电脑上运行 |
| `hardware.system-disk-type` | Intel RST / VMD 的 RAID 模式下能否对应到物理盘；eMMC 小本报的总线类型 | 在开了 VMD 的新笔记本和 eMMC 小本上运行 |
| `hardware.memory-size` | 板载内存、部分 BIOS 报的插槽数不准；虚拟机里插槽数是 0 | 在板载内存的笔记本上对照任务管理器 |
| `hardware.battery-health` | 接了 UPS 的台式机会不会把 UPS 当成电池 | 在接 UPS 的台式机上运行 |
| `boot.last-boot-duration` | 开着快速启动时，「关机再开机」会不会记录事件 100 | 开快速启动，关机再开机后运行 |
| `system.reliability-recent` | 精简系统可能关掉了可靠性数据收集，这时会误显示「正常」 | 在关闭了 RacTask 计划任务的电脑上运行 |
| `disk.error-events` | 直接拔 U 盘可能留下 51 / 153 事件而误报（文字里已经提醒） | 拔一次正在读写的 U 盘后运行 |
| `system.os-support` | Win11 26H2（build 26300）正式发布后要把日期补进表里；国内能否在「Windows 更新」里完成免费 ESU 登记 | 26H2 发布后更新表格；在 Win10 22H2 家庭版上走一遍 ESU 登记 |

## 修复

| 修复 | 要确认什么 | 怎么验证 |
| --- | --- | --- |
| `network.proxy-off` | 现在会同时改 `ProxyEnable` 和 WinINet 实际读取的二进制值 `Internet Settings\Connections\DefaultConnectionSettings`（清掉标志 0x02、计数器加 1，其余字节不动；这个格式是社区逆向出来的）。还要确认两件事：改完是否立刻生效（脚本没法通知正在运行的程序，已打开的浏览器可能还在用旧设置）；重启电脑后是否保持关闭（没有被 `SavedLegacySettings` 或别的程序改回来） | 用梯子制造「代理残留」后执行修复：已打开的浏览器能否马上上网、重开浏览器后能否上网、「设置 → 网络 → 代理」里显示什么；重启电脑后再运行 `network.proxy-dead`，看 `legacy_proxy_enabled`、`blob_proxy_enabled` 是否仍为「否」 |
| `start.disable-web-search` | 微软文档列出的适用版本是专业版、企业版、教育版，社区测试说家庭版直接写注册表也有效；只重启资源管理器够不够 | 在家庭中文版上执行，注销再登录后在开始菜单搜索 |
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
