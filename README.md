# JianerCLI

用于管理本地 [JianerCore](../JianerCore) / Jianer_QQ_bot 实例的 Rust CLI/TUI 工具。

## 构建

```bash
cargo build --release
```

需要 Rust 1.75 或更新版本。

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
jianer-cli plugin install --all    全部插件重装到最新版本
jianer-cli plugin remove <id>      删除插件（默认清理无用依赖）
jianer-cli plugin remove --no-deps <id>
```

全局选项：`--market-url <URL>` 覆盖市场索引地址，`--json` 输出机器可读 JSON，`--no-color` 关闭颜色。

## 插件安装

插件本体不是 pip 包。市场 Release 提供 zip，JianerCLI 会：

1. 下载 zip 并校验 market index 声明的 SHA-256；
2. 安全解压到 bot 的 `plugins/`；
3. 写入 `market.json` / `Xxx.market.json` 记录版本；
4. 下载完整后，使用 bot 项目 `.venv` 的 Python 执行 `python -m pip install`，安装插件声明的 `pipDependencies`。

`__plugin_meta__.requires` 声明的插件依赖会级联安装；内置 `jianerbot-plugin-alconna` 视为已满足。未检测到项目虚拟环境时会明确提示，并使用 `JIANER_PYTHON` 或系统 Python 作为回退解释器。

## 智慧市场

市场自动化已迁移至同级目录 `../Jianer_Plugin_Market/`，包含索引 schema、索引生成脚本和 GitHub Actions。CLI 默认读取约定的索引 URL，并支持通过全局 `--market-url`、`JIANER_MARKET_URL` 或用户级配置覆盖。

## 项目行为

`create` 根据 JianerCore 官方最小 Bot 结构生成脚手架；`config` 修改 `config.json` 时保留未知字段并先生成 `.json.bak` 备份；`run` 和插件 PyPI 依赖安装优先使用项目 `.venv`。插件版本由市场索引与安装时写入的 sidecar 记录提供。
