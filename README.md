# JianerCLI

使用 Python 3.12 编写的 JianerCore / Jianer_QQ_bot 项目管理 CLI。

## 安装与运行

```bash
python3.12 -m pip install -e .
jianer-cli --help
# 或开发时
python3.12 -m jianer_cli --help
```

## 命令

```text
jianer-cli create [core|bot]       创建项目
jianer-cli info                    项目体检（doctor 为同义命令）
jianer-cli config                  配置 bot config.json
jianer-cli run                     使用项目 .venv 启动 bot
jianer-cli update-conf             配置 main/NEXT-PREVIEW/dev 渠道
jianer-cli upgrade                 更新 bot 项目
jianer-cli plugin search <query>   搜索插件市场
jianer-cli plugin show [-r|-l] ID  查看远端/本地插件
jianer-cli plugin list             列出本地插件
jianer-cli plugin outdated         检查插件更新
jianer-cli plugin install ID       安装插件和插件依赖
jianer-cli plugin install -U ID    升级安装
jianer-cli plugin install --all    全量重装
jianer-cli plugin remove ID        删除插件
```

插件本体以 Release zip 安装，SHA-256 校验后解压进 `plugins/`。只有市场索引中的 `pipDependencies` 会调用 bot 的 `.venv/bin/python -m pip install`；插件本身不是 pip 包。插件元数据由 Python AST 静态解析，不 import 插件。

市场 index.json 地址可通过 `--market-url` 或 `JIANER_MARKET_URL` 覆盖。默认地址指向现有旧版 Jianer_Plugins_Index 仓库；旧市场尚未提供 JianerCLI schema v1 的 `index.json`，需要在测试时传入兼容索引地址。

## 智慧市场自动化

本地插件市场脚本已迁移至同级目录 `../Jianer_Plugin_Market/`。
