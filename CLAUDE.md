# CLAUDE.md

This file provides guidance to Сlaude Code (claude.ai/code) when working with code in this repository.

## 这个仓库是什么

JianerCLI 是一个**规划中的 Rust CLI/TUI 项目**（代码尚未开工），用于管理本地 JianerCore / Jianer_QQ_bot 实例。目前仓库包含两部分：

- `jianer-cli实现方案.md` — 项目的需求与命令设计文档（`create` / `config` / `plugin` / `upgrade` / `update-conf`），是所有功能的唯一权威来源，动手前先读它。
- `market/` — 「智慧市场」（SR思锐的静态插件市场）的索引 schema 与自动化，设计为放入插件 monorepo 使用，与本仓库的 Rust 代码无构建关系。

## 相邻仓库（本目录之外，经常需要参考）

- `../JianerCore` — Python 机器人框架。插件元数据 `PluginMetadata`（name/description/usage/requires）定义在 `jianer/plugins/metadata.py`；插件开发文档在 `documents/`。
- `../Jianer_Canary/Jianer_QQ_bot` — 基于 JianerCore 的 bot 实例。`plugins/` 是插件形态的真实样本：单文件插件（`Xxx.py`）与包插件（`Xxx/` 目录，入口 `setup.py`，模块级 `__plugin_meta__`）。

## 智慧市场架构（market/）

纯静态方案，无服务端：插件通过 PR 进入插件 monorepo 的 `plugins/`，合并后 GitHub Actions 生成 `index.json` 并为每个新版本发 GitHub Release。

- **分发单位**：每个 `pluginId@version` 一个 Release tag，asset 为 zip（zip 根 = 插件目录名/文件名，解压进 bot 的 `plugins/` 即安装）。
- **下载量**：来自 GitHub API `/releases/tags/{tag}` 的 asset `download_count`（静态方案下唯一可行的计数方式，勿在索引里加 downloads 字段）。
- **插件元数据双来源**：`__plugin_meta__` 由 `generate_index.py` 用 AST 静态解析（**不 import 插件**，CI 无需装 bot 依赖）；`version` 等市场字段来自插件目录的 `market.json`（单文件插件为同级 `Xxx.market.json`），version 必填。
- **幂等性**：已发布过的 `id@version` 复用旧 `index.json` 里的 size/sha256，不重新打包、不重复发 Release。修改这套逻辑时必须保持这一点。

文件对应关系：

- `market-index.schema.json` — index.json 的 JSON Schema（2020-12），字段语义都写在 description 里。
- `scripts/generate_index.py` — `check` 模式（PR 校验，只读）/ `index` 模式（生成 index.json + 打包 zip 到 `dist/` + 输出 `dist/releases.json` 供 workflow 发 Release）。
- `build-market-index.yml` / `pr-check.yml` — 部署到插件 monorepo 的 `.github/workflows/`；schema 和脚本放该仓库根目录（workflow 按相对路径引用）。

## 常用命令

```bash
# PR 校验模式（只检查，退出码非零即失败）
python3 market/scripts/generate_index.py check <repo_root>

# 生成 index.json 并打包新版本 zip
python3 market/scripts/generate_index.py index <repo_root>
```

本地测试可把 `../Jianer_Canary/Jianer_QQ_bot/plugins` 拷到临时目录并为每个插件补 `market.json`（`{"version": "x.y.z"}`）。本机没有 pip/jsonschema（Arch 系统 Python），schema 校验只在 CI 里跑。

## 约束

- schema、脚本、workflow 三者必须同步演进：改 schema 字段就要同时改 `generate_index.py` 的输出和 workflow 的校验/发布步骤。
- 未来的 Rust CLI 中，`plugin` 子命令的行为（`--r/--l`、`--no-deps`、`-U --all`、模糊匹配）以实现方案文档为准；更新渠道展示名（`main (Fixed Release)`、`NEXT-PREVIEW (Curated Rolling Release)`、`dev (Rolling Release)` 含 10 秒免责声明）也在文档中有明确规定。
