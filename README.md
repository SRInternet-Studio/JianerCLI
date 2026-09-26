# JianerCLI

用于管理本地 [JianerCore](../JianerCore) / Jianer_QQ_bot 实例的 Rust CLI/TUI 工具。

## 构建

```bash
cargo build --release
```

当前支持 Rust 1.75+，推荐最新 stable。

## 命令

```text
jianer-cli create [core|bot]       创建项目
jianer-cli info                    项目体检（doctor 是别名）
jianer-cli config                  交互式配置当前 Bot
jianer-cli run                     使用项目 .venv 启动 Bot
jianer-cli update-conf             配置 main/NEXT-PREVIEW/dev 更新渠道
jianer-cli upgrade                 拉取配置渠道的最新 Bot 代码

jianer-cli plugin search <query>   搜索智慧市场
jianer-cli plugin show <id>        查看插件（本地优先）
jianer-cli plugin show -r <id>     查看远端插件
jianer-cli plugin list             列出本地插件
jianer-cli plugin outdated         检查插件更新
jianer-cli plugin install <id>     下载、校验、解压并安装插件
jianer-cli plugin install -U <id>  升级安装插件
jianer-cli plugin install --all x  全部插件重装到最新版本
jianer-cli plugin remove <id>      删除插件（默认清理无用依赖）
jianer-cli plugin remove --no-deps <id>
```

## 插件安装语义

插件本体不是 pip 包。市场 Release 提供 zip，JianerCLI 会：

1. 下载 zip；
2. 校验 market index 声明的 SHA-256；
3. 安全解压到 bot 的 `plugins/`；
4. 写入 `market.json` / `Xxx.market.json` 记录版本；
5. 下载完整后用 bot 项目 `.venv` 的 `python -m pip install` 安装插件声明的 `pipDependencies`。

依赖插件通过 `__plugin_meta__.requires` 级联安装，内置
`jianerbot-plugin-alconna` 视为已满足。

## 市场迁移

插件市场自动化已经迁移至相邻目录：

```text
../Jianer_Plugin_Market/
├── market-index.schema.json
├── scripts/generate_index.py
├── build-market-index.yml
└── pr-check.yml
```

CLI 默认读取约定的 `index.json` 地址，也可以用全局 `--market-url` 或
`JIANER_MARKET_URL` 覆盖。

## 说明

`create` 生成 JianerCore 官方最小 Bot 结构；`config` 保留 `config.json`
中的未知字段，并在写回前生成 `.json.bak`；`run`、插件 pip 安装都优先使用
项目 `.venv`，避免污染系统 Python。
