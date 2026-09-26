# 电脑小药箱（暂定名）

免费、开源的 Windows 电脑维护工具。

- 普通人可以按症状点「上不了网」「C 盘满了」「打印机连不上」，自己查出原因、修好电脑。
- 帮亲友修电脑的「懂哥」可以装在 U 盘里带着走，批量处理。

> **项目处于早期开发阶段（v0.0：只读体检），还不适合普通用户使用。** 名称还是暂定的，见[计划书第十三节](docs/plan.md#十三待拍板的事项附建议)。

## 原则

1. **先诊断，后动手**：先说清楚哪里有问题，征得同意再改。
2. **能撤销**：每一项修改都记进修改日志，可以逐项撤销；不能撤销的，事先用红字说明。
3. **说人话**：每个功能都用一句大白话说明「会做什么、有什么影响」。
4. **零打扰**：没有广告、捆绑、遥测和账号；不改浏览器主页，不开机自启，不常驻后台。
5. **可验证**：源码公开、构建过程公开、安装包带签名、官网公布校验值。
6. **只用系统自带或官方来源**。
7. **不制造焦虑**：没问题就说「一切正常」，不打分。
8. **知道边界**：修不了的问题直接说明，并告诉你下一步该找谁。
9. **用户文件零容忍**：凡是会删除你的文件的操作，都逐项确认，默认不勾选。
10. **优先用系统自己的机制**：即使卸载了小药箱，改过的设置在系统里也看得到、改得回。

## 我们不做什么

我们不做以下这些，每一条的原因都写在[计划书第五节](docs/plan.md#五我们不做什么以及为什么)：

- 激活工具
- 广告和捆绑
- 遥测
- 关闭 Defender
- 永久禁用更新
- 精简或删除系统组件
- 第三方修改版系统镜像
- 驱动库
- 杀毒
- 「一键加速」
- 关闭 UAC 或 SmartScreen
- 用 hosts 屏蔽微软
- 远程控制

## 文档

- [执行计划书 v1.1](docs/plan.md)
- [竞品调研与取长补短](docs/competitive-analysis.md)
- [架构与数据格式规范](docs/architecture.md)
- [功能编写指南](docs/feature-authoring.md)：写一个新的检测或修复
- [真机验证清单](docs/real-machine-checklist.md)：需要在真实电脑上确认的地方
- [参与贡献](CONTRIBUTING.md)

## 开发

需要：[Rust](https://rustup.rs/)（版本由 `rust-toolchain.toml` 固定）、Node.js 24 和 pnpm（`corepack enable`）。打包安装包需要在 Windows 上进行。

```sh
cargo test                              # 引擎测试；装了 pwsh 时还会跑脚本宿主协议测试
cargo run -p medkit-data -- check       # 校验 catalog/ 和 scripts/
pnpm install
pnpm --dir app dev                      # 在浏览器里开发界面（用示例数据，不碰系统）
pnpm tauri dev                          # Windows 上真实运行：要在「以管理员身份运行」的终端里执行
pnpm tauri build                        # Windows 上：生成安装包
```

目录结构：

| 目录 | 内容 |
| --- | --- |
| `catalog/` | 检测、修复、症状、检测清单（YAML） |
| `scripts/` | PowerShell 脚本（只用 ASCII，兼容 Windows PowerShell 5.1） |
| `crates/medkit-core/` | 引擎：数据校验、执行、修改日志与撤销、脚本宿主 |
| `crates/medkit-data/` | 数据工具：校验、生成 JSON Schema、打包 |
| `src-tauri/` | 桌面外壳 |
| `app/` | 界面（Vue 3） |

## 许可证

代码、`catalog/` 和 `scripts/` 使用 [GPL-3.0-or-later](LICENSE)；`data/` 下的知识数据使用 [CC BY-SA 4.0](data/LICENSE)。
