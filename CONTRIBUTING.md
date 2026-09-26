# 参与贡献

谢谢你愿意帮忙！先读这三份文档：

1. [执行计划书](docs/plan.md)：做什么、不做什么、为什么。**第五节「不做什么」是硬性要求**，违反的 PR 不会合并。
2. [架构与数据格式规范](docs/architecture.md)：数据文件、脚本和接口的写法。
3. [功能编写指南](docs/feature-authoring.md)：一步步写一个新的检测或修复。

## 最容易上手的贡献

- **真机验证**：[真机验证清单](docs/real-machine-checklist.md)里的每一项，都只需要一台对应的电脑和一点耐心，不用写代码。
- **数据**：同义词、错误码解释、弹窗知识库、品牌 BIOS 按键表。不用写代码。
- **图文指引**：给 `catalog/symptoms/` 里的症状写手动排查步骤（`maturity: guide`）。
- **检测和修复**：一个 YAML，加一两个 PowerShell 脚本。

## 提交前自查

```sh
cargo run -p medkit-data -- check      # 校验 catalog、检查脚本
cargo test                             # 引擎测试
pnpm install && pnpm --dir app build   # 界面能构建
```

- 每个检测、修复都要附**来源**（`references`），优先引用微软官方文档。
- 脚本只能有 ASCII 字符；界面上的文字写在 YAML 里。
- 修改系统的功能必须能撤销；不能撤销的要写明原因（`irreversible_reason`）。

## 签名确认（DCO）

我们用 [Developer Certificate of Origin](https://developercertificate.org/) 代替 CLA。每个提交都要带上：

```
Signed-off-by: 你的名字 <你的邮箱>
```

用 `git commit -s` 会自动加上。它表示你有权按本项目的许可证提交这些内容。

## 许可证

- 代码、`catalog/`、`scripts/` 使用 [GPL-3.0-or-later](LICENSE)。
- `data/` 下的知识数据使用 [CC BY-SA 4.0](data/LICENSE)，方便其他网站和工具复用。

## 行为准则

友善、就事论事。帮人修电脑的人都值得被耐心对待。
