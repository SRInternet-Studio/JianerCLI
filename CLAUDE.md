# CLAUDE.md

## 项目说明

JianerCLI 使用 Python 3.12，管理本地 JianerCore / Jianer_QQ_bot 项目。
需求与命令规范见 `jianer-cli实现方案.md`。智慧市场自动化位于同级
`../Jianer_Plugin_Market/`，与 CLI 包分离。

## 关键约定

- 插件本体是市场 Release zip，不是 pip 包；只有索引 `pipDependencies`
  是 PyPI 依赖，安装插件后必须用 Bot 项目 `.venv` Python 执行 pip。
- `PluginMetadata` 通过 Python AST 静态解析，不 import 插件代码。
- Bot `config.json` 更新必须保留未知字段，并先备份原文件。
- 插件市场索引格式以 `../Jianer_Plugin_Market/market-index.schema.json` 为准。
- 插件包解压必须阻止路径穿越；下载后必须校验 SHA-256。

## 开发与测试

```bash
python3.12 -m unittest discover -s tests -v
python3.12 -m compileall -q jianer_cli
python3.12 -m pip install -e .
```
