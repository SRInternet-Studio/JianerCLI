//! JianerCLI 主入口。

mod config;
mod error;
mod installer;
mod local;
mod market;
mod project;
#[cfg(test)]
mod tests;
mod ui;
mod update;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use inquire::{Select, Text};
use serde_json::json;

use crate::config::{split_list, BotConfigFile, UserConfig};
use crate::error::{CliError, Result};
use crate::installer::Installer;
use crate::local::{match_local, scan_plugins, LocalPlugin};
use crate::market::{MarketClient, MarketPlugin, MarketSource};
use crate::project::{Project, ProjectKind};
use crate::ui::{human_bytes, table, Style};
use crate::update::{configured_channel, current_branch, parse_channel};

const ABOUT: &str = "管理本地 JianerCore / Jianer_QQ_bot 实例的 Rust CLI";

#[derive(Debug, Parser)]
#[command(name = "jianer-cli", version, about = ABOUT, disable_help_subcommand = true)]
struct Cli {
    #[arg(long, global = true, help = "关闭颜色输出")]
    no_color: bool,
    #[arg(long, global = true, help = "只输出错误")]
    quiet: bool,
    #[arg(long, global = true, help = "以 JSON 输出结果")]
    json: bool,
    #[arg(
        long,
        global = true,
        value_name = "URL",
        help = "覆盖智慧市场 index.json 地址"
    )]
    market_url: Option<String>,
    #[command(subcommand)]
    command: Option<CommandKind>,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// 创建 JianerCore 项目或基于 JianerCore 的 Bot 项目。
    Create(CreateArgs),
    /// 配置当前 Jianer_QQ_bot 实例。
    Config(ConfigArgs),
    /// 管理插件市场与本地插件。
    #[command(subcommand)]
    Plugin(PluginCommand),
    /// 更新当前 Bot 项目。
    Upgrade(UpgradeArgs),
    /// 配置 Bot 更新渠道。
    UpdateConf(UpdateConfArgs),
    /// 检测项目环境与常见问题。
    Info,
    /// `info` 的别名，保留给脚本与用户习惯。
    Doctor,
    /// 启动当前 Bot 实例。
    Run(RunArgs),
}

#[derive(Debug, Args)]
struct CreateArgs {
    #[arg(value_enum)]
    kind: Option<CreateKind>,
    #[arg(short, long, value_name = "PATH")]
    path: Option<PathBuf>,
    #[arg(long, help = "不进入交互式向导")]
    non_interactive: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CreateKind {
    Core,
    Bot,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[arg(long, help = "直接修改协议：OneBot/Milky/Kritor/Feishu")]
    protocol: Option<String>,
    #[arg(long, value_name = "QQ", help = "设置 owner 列表，逗号分隔")]
    owner: Option<String>,
    #[arg(long, value_name = "NAME", help = "设置 bot 名称")]
    bot_name: Option<String>,
    #[arg(long, value_name = "HOST", help = "设置当前协议 host")]
    host: Option<String>,
    #[arg(long, value_name = "PORT", help = "设置当前协议 port")]
    port: Option<u64>,
    #[arg(long, help = "不进入交互式配置")]
    non_interactive: bool,
}

#[derive(Debug, Subcommand)]
enum PluginCommand {
    /// 在智慧市场搜索插件。
    Search(SearchArgs),
    /// 查看插件详情，默认本地优先。
    Show(ShowArgs),
    /// 安装插件及其依赖。
    Install(InstallArgs),
    /// 删除插件。
    Remove(RemoveArgs),
    /// 列出本地已安装插件。
    List,
    /// 列出有新版本的插件。
    Outdated,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: String,
    #[arg(long, help = "忽略缓存，重新拉取索引")]
    refresh: bool,
    #[arg(long, help = "显示已弃用插件")]
    include_deprecated: bool,
}

#[derive(Debug, Args)]
struct ShowArgs {
    plugin_id: String,
    #[arg(short = 'r', long, help = "只查询远端智慧市场")]
    remote: bool,
    #[arg(short = 'l', long, help = "只查询本地插件")]
    local: bool,
    #[arg(long, help = "忽略缓存")]
    refresh: bool,
}

#[derive(Debug, Args)]
struct InstallArgs {
    #[arg(value_name = "PLUGIN_ID", required_unless_present = "all")]
    plugin_id: Option<String>,
    #[arg(short = 'U', help = "升级安装；不加时已安装插件不会覆盖")]
    upgrade: bool,
    #[arg(long, help = "全部插件重新安装到最新版本")]
    all: bool,
    #[arg(long, help = "只显示将执行的操作，不修改文件或运行 pip")]
    dry_run: bool,
    #[arg(long, help = "忽略索引缓存")]
    refresh: bool,
}

#[derive(Debug, Args)]
struct RemoveArgs {
    plugin_id: String,
    #[arg(long = "no-deps", help = "保留该插件声明的依赖")]
    no_deps: bool,
}

#[derive(Debug, Args)]
struct UpgradeArgs {
    #[arg(long, help = "覆盖已配置的渠道")]
    channel: Option<String>,
    #[arg(long, help = "只显示命令，不执行")]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct UpdateConfArgs {
    #[arg(long, help = "跳过交互，使用 --channel 或 main")]
    non_interactive: bool,
    #[arg(long)]
    channel: Option<String>,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(last = true, help = "传给 main.py 的参数")]
    args: Vec<String>,
    #[arg(long, help = "只打印启动命令")]
    dry_run: bool,
}

fn main() {
    let cli = Cli::parse();
    let style = Style::detect(cli.no_color, cli.quiet, cli.json);
    let result = dispatch(&cli);
    if let Err(error) = result {
        style.error(error.to_string());
        std::process::exit(1);
    }
}

fn dispatch(cli: &Cli) -> Result<()> {
    let style = Style::detect(cli.no_color, cli.quiet, cli.json);
    let Some(command) = &cli.command else {
        Cli::command()
            .print_help()
            .map_err(|e| CliError::Network(e.to_string()))?;
        println!();
        return Ok(());
    };
    match command {
        CommandKind::Create(args) => create(args, &style),
        CommandKind::Config(args) => config(args, &style),
        CommandKind::Plugin(command) => plugin(command, cli.market_url.as_deref(), &style),
        CommandKind::Upgrade(args) => upgrade(args, &style),
        CommandKind::UpdateConf(args) => update_conf(args, &style),
        CommandKind::Info | CommandKind::Doctor => info(&style),
        CommandKind::Run(args) => run(args, &style),
    }
}

fn create(args: &CreateArgs, style: &Style) -> Result<()> {
    let kind = match args.kind {
        Some(k) => k,
        None if args.non_interactive => CreateKind::Bot,
        None => Select::new(
            "选择要创建的项目类型",
            vec!["基于 JianerCore 的 Bot 项目", "JianerCore 项目"],
        )
        .prompt()
        .map_err(|_| CliError::Cancelled)?
        .into(),
    };
    let root = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir().unwrap_or_default().join(root)
    };
    std::fs::create_dir_all(&root)
        .map_err(|e| CliError::io(format!("创建 {}", root.display()), e))?;
    match kind {
        CreateKind::Core => create_core(&root, style),
        CreateKind::Bot => create_bot(&root, style),
    }
}

impl From<&str> for CreateKind {
    fn from(value: &str) -> Self {
        if value.starts_with("JianerCore") {
            CreateKind::Core
        } else {
            CreateKind::Bot
        }
    }
}

fn create_core(root: &std::path::Path, style: &Style) -> Result<()> {
    let main = root.join("main.py");
    if !main.exists() {
        std::fs::write(
            &main,
            "from jianer import Client\n\nwith Client() as client:\n    client.run()\n",
        )
        .map_err(|e| CliError::io("写入 main.py", e))?;
    }
    std::fs::create_dir_all(root.join("plugins")).map_err(|e| CliError::io("创建 plugins", e))?;
    write_minimal_requirements(root)?;
    style.success(format!("已创建 JianerCore 项目：{}", root.display()));
    style.hint(
        "下一步：python -m venv .venv && .venv/bin/python -m pip install -r requirements.txt",
    );
    Ok(())
}

fn create_bot(root: &std::path::Path, style: &Style) -> Result<()> {
    let config = root.join("config.json");
    if !config.exists() {
        std::fs::write(&config, serde_json::to_string_pretty(&json!({
            "protocol": "OneBot", "owner": [], "black_list": [], "silents": [],
            "connections": {"OneBot": {"mode": "FWS", "host": "127.0.0.1", "port": 5004, "listener_host": "127.0.0.1", "listener_port": 8081, "retries": 5, "token": null, "auth": null}},
            "log_level": "INFO", "log_use_nf": false, "uin": 0, "max_workers": 25, "others": {}
        })).map_err(|e| CliError::Network(e.to_string()))? + "\n")
            .map_err(|e| CliError::io("写入 config.json", e))?;
    }
    let main = root.join("main.py");
    if !main.exists() {
        std::fs::write(&main, "from cfgr.manager import Serializers\nfrom jianer import Client, configurator\n\nconfigurator.BotConfig.load_from('config.json', Serializers.JSON, 'jianer-bot')\nfrom jianer.adapters import builtins as adapters\nadapters.load_configured()\nwith Client() as client:\n    result = client.load_plugins('plugins')\n    if result.failed:\n        raise RuntimeError(result.failed)\n    client.run()\n")
            .map_err(|e| CliError::io("写入 main.py", e))?;
    }
    std::fs::create_dir_all(root.join("plugins")).map_err(|e| CliError::io("创建 plugins", e))?;
    write_minimal_requirements(root)?;
    style.success(format!("已创建 Jianer_QQ_bot 项目：{}", root.display()));
    style.hint("下一步：创建 .venv 后运行 jianer-cli config 配置连接协议");
    Ok(())
}

fn write_minimal_requirements(root: &std::path::Path) -> Result<()> {
    let path = root.join("requirements.txt");
    if !path.exists() {
        std::fs::write(path, "jianer-bot\n")
            .map_err(|e| CliError::io("写入 requirements.txt", e))?;
    }
    Ok(())
}

fn config(args: &ConfigArgs, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let path = project.config_path()?;
    let mut file = BotConfigFile::load(path)?;
    let protocol = args.protocol.clone().or_else(|| file.get_str("protocol"));
    if let Some(p) = &args.protocol {
        file.set_str("protocol", p)?;
    }
    if let Some(owner) = &args.owner {
        file.set_string_list("owner", &split_list(owner))?;
    }
    if let Some(name) = &args.bot_name {
        file.set_other_str("bot_name", name)?;
    }
    let proto = protocol.unwrap_or_else(|| "OneBot".to_string());
    if let Some(host) = &args.host {
        file.set_connection(&proto, "host", json!(host))?;
    }
    if let Some(port) = args.port {
        file.set_connection(&proto, "port", json!(port))?;
    }
    if args.non_interactive
        || args.protocol.is_some()
        || args.owner.is_some()
        || args.bot_name.is_some()
        || args.host.is_some()
        || args.port.is_some()
    {
        file.save()?;
        style.success(format!("已更新 {}", path.display()));
        return Ok(());
    }
    let protocols = vec!["OneBot", "Milky", "Kritor", "Feishu"];
    let selected = Select::new("选择当前协议", protocols)
        .prompt()
        .map_err(|_| CliError::Cancelled)?;
    file.set_str("protocol", selected)?;
    let host = Text::new("协议 host")
        .with_default(
            file.connection(selected)
                .and_then(|m| m.get("host"))
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1"),
        )
        .prompt()
        .map_err(|_| CliError::Cancelled)?;
    let port = Text::new("协议 port")
        .with_default(
            &file
                .connection(selected)
                .and_then(|m| m.get("port"))
                .and_then(|v| v.as_u64())
                .unwrap_or(5004)
                .to_string(),
        )
        .prompt()
        .map_err(|_| CliError::Cancelled)?;
    file.set_connection(selected, "host", json!(host))?;
    file.set_connection(selected, "port", json!(port.parse::<u64>().unwrap_or(5004)))?;
    let bot_name = Text::new("Bot 名称（可留空）")
        .with_default(
            file.value()
                .get("others")
                .and_then(|o| o.get("bot_name"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        )
        .prompt()
        .map_err(|_| CliError::Cancelled)?;
    if !bot_name.is_empty() {
        file.set_other_str("bot_name", &bot_name)?;
    }
    file.save()?;
    style.success("配置已保存，并生成了 config.json.bak 备份");
    Ok(())
}

fn market_client(override_url: Option<&str>) -> Result<(UserConfig, MarketClient)> {
    let user = UserConfig::load();
    let source = MarketSource::resolve(override_url, &user);
    let client = MarketClient::new(source, UserConfig::cache_dir())?;
    Ok((user, client))
}

fn plugin(command: &PluginCommand, market_url: Option<&str>, style: &Style) -> Result<()> {
    match command {
        PluginCommand::List => plugin_list(style),
        PluginCommand::Search(args) => plugin_search(args, market_url, style),
        PluginCommand::Show(args) => plugin_show(args, market_url, style),
        PluginCommand::Install(args) => plugin_install(args, market_url, style),
        PluginCommand::Remove(args) => plugin_remove(args, style),
        PluginCommand::Outdated => plugin_outdated(market_url, style),
    }
}

fn plugin_list(style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let plugins = scan_plugins(&project.plugins_dir)?;
    if style.json {
        return style.emit_json(
            &serde_json::to_value(&plugins).map_err(|e| CliError::Network(e.to_string()))?,
        );
    }
    let rows = plugins
        .iter()
        .map(|p| {
            vec![
                p.id_label().to_string(),
                p.market_version
                    .clone()
                    .unwrap_or_else(|| "未知".to_string()),
                p.kind.label().to_string(),
                if p.disabled { "禁用" } else { "启用" }.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    table(style, &["插件 ID", "版本", "类型", "状态"], &rows);
    style.info(format!("共 {} 个插件", plugins.len()));
    Ok(())
}

fn plugin_search(args: &SearchArgs, market_url: Option<&str>, style: &Style) -> Result<()> {
    let (_user, client) = market_client(market_url)?;
    let index = client.index(args.refresh)?;
    let mut matches: Vec<&MarketPlugin> = index
        .plugins
        .iter()
        .filter(|p| p.matches(&args.query) && (args.include_deprecated || !p.deprecated))
        .collect();
    matches.sort_by_key(|p| p.fuzzy_score(&args.query).unwrap_or(99));
    if style.json {
        return style.emit_json(
            &serde_json::to_value(&matches).map_err(|e| CliError::Network(e.to_string()))?,
        );
    }
    let rows = matches
        .iter()
        .map(|p| {
            vec![
                p.id.clone(),
                p.version.clone(),
                p.name.clone(),
                p.description.clone(),
            ]
        })
        .collect::<Vec<_>>();
    table(style, &["插件 ID", "版本", "名称", "描述"], &rows);
    style.info(format!("找到 {} 个插件", matches.len()));
    Ok(())
}

fn plugin_show(args: &ShowArgs, market_url: Option<&str>, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let locals = scan_plugins(&project.plugins_dir)?;
    let local_matches = match_local(&locals, &args.plugin_id);
    if args.local && args.remote {
        return Err(CliError::Network(
            "--local 与 --remote 不能同时使用".to_string(),
        ));
    }
    if !args.remote && !local_matches.is_empty() {
        return show_local(local_matches[0], style);
    }
    let (_user, client) = market_client(market_url)?;
    let index = client.index(args.refresh)?;
    let remote = resolve_remote(&index.plugins, &args.plugin_id)?;
    show_remote(&client, &index, remote, style)
}

fn show_local(plugin: &LocalPlugin, style: &Style) -> Result<()> {
    if style.json {
        return style.emit_json(
            &serde_json::to_value(plugin).map_err(|e| CliError::Network(e.to_string()))?,
        );
    }
    style.section(&format!("本地插件：{}", plugin.id_label()));
    style.kv("路径", plugin.path.display().to_string());
    style.kv("类型", plugin.kind.label());
    style.kv("版本", plugin.market_version.as_deref().unwrap_or("未知"));
    if let Some(meta) = &plugin.meta {
        style.kv("描述", &meta.description);
        style.kv(
            "依赖",
            if meta.requires.is_empty() {
                "无".to_string()
            } else {
                meta.requires.join(", ")
            },
        );
    }
    if let Some(err) = &plugin.parse_error {
        style.warn(err);
    }
    Ok(())
}

fn show_remote(
    client: &MarketClient,
    index: &market::MarketIndex,
    plugin: &MarketPlugin,
    style: &Style,
) -> Result<()> {
    if style.json {
        return style.emit_json(
            &serde_json::to_value(plugin).map_err(|e| CliError::Network(e.to_string()))?,
        );
    }
    style.section(&format!("市场插件：{}", plugin.id));
    style.kv("名称", &plugin.name);
    style.kv("版本", &plugin.version);
    style.kv("类型", plugin.plugin_type.label());
    style.kv("描述", &plugin.description);
    style.kv(
        "作者",
        if plugin.authors.is_empty() {
            "未知".to_string()
        } else {
            plugin.authors.join(", ")
        },
    );
    style.kv(
        "依赖",
        if plugin.requires.is_empty() {
            "无".to_string()
        } else {
            plugin.requires.join(", ")
        },
    );
    style.kv(
        "PyPI 依赖",
        if plugin.pip_dependencies.is_empty() {
            "无".to_string()
        } else {
            plugin.pip_dependencies.join(", ")
        },
    );
    style.kv(
        "下载包",
        format!(
            "{} ({})",
            plugin.release.asset,
            human_bytes(plugin.release.size)
        ),
    );
    let counts = client.download_counts(&index.repository, &plugin.release.tag);
    if let Some(n) = counts.get(&plugin.release.asset) {
        style.kv("下载量", n.to_string());
    }
    if plugin.deprecated {
        style.warn("该插件已弃用");
    }
    Ok(())
}

fn resolve_remote<'a>(plugins: &'a [MarketPlugin], query: &str) -> Result<&'a MarketPlugin> {
    let mut candidates: Vec<(&MarketPlugin, usize)> = plugins
        .iter()
        .filter_map(|p| p.fuzzy_score(query).map(|score| (p, score)))
        .collect();
    if candidates.is_empty() {
        return Err(CliError::PluginNotFound(query.to_string()));
    }
    candidates.sort_by_key(|(_, score)| *score);
    let best = candidates[0].1;
    let same: Vec<&MarketPlugin> = candidates
        .into_iter()
        .filter(|(_, score)| *score == best)
        .map(|(p, _)| p)
        .collect();
    if same.len() > 1 {
        return Err(CliError::AmbiguousPlugin {
            query: query.to_string(),
            candidates: same
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(same[0])
}

fn plugin_install(args: &InstallArgs, market_url: Option<&str>, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let (user, client) = market_client(market_url)?;
    let index = client.index(args.refresh)?;
    let roots: Vec<&MarketPlugin> = if args.all {
        index.plugins.iter().filter(|p| !p.deprecated).collect()
    } else {
        vec![resolve_remote(
            &index.plugins,
            args.plugin_id.as_deref().unwrap_or_default(),
        )?]
    };
    let mut installed: BTreeSet<String> = scan_plugins(&project.plugins_dir)?
        .into_iter()
        .filter_map(|p| p.id)
        .collect();
    // -U 强制把用户指定的根插件从“已安装”集合移除；否则依赖解析会把它
    // 视为已满足，导致升级命令静默地什么也不做。
    if args.upgrade && !args.all {
        for root in &roots {
            installed.remove(&root.id);
        }
    }
    let builtins = local::builtin_ids();
    let mut order = Vec::new();
    for root in roots {
        let deps = local::resolve_dependencies(
            &root.id,
            &|id| {
                index
                    .plugins
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.requires.clone())
            },
            &installed,
            &builtins,
        )?;
        for id in deps {
            if !order.contains(&id) {
                order.push(id);
            }
        }
    }
    if args.all {
        order = index
            .plugins
            .iter()
            .filter(|p| !p.deprecated)
            .map(|p| p.id.clone())
            .collect();
    }
    for id in order {
        let plugin = index
            .plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| CliError::PluginNotFound(id.clone()))?;
        let installer = Installer {
            project: &project,
            market: &client,
            user: &user,
        }
        .with_repository(&index.repository);
        installer.install(plugin, args.upgrade || args.all, args.dry_run)?;
        if !args.dry_run {
            style.success(format!("已安装 {} {}", plugin.id, plugin.version));
        }
    }
    Ok(())
}

fn plugin_remove(args: &RemoveArgs, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let plugins = scan_plugins(&project.plugins_dir)?;
    let candidates = match_local(&plugins, &args.plugin_id);
    if candidates.is_empty() {
        return Err(CliError::PluginNotInstalled(args.plugin_id.clone()));
    }
    if candidates.len() > 1 {
        return Err(CliError::AmbiguousPlugin {
            query: args.plugin_id.clone(),
            candidates: candidates
                .iter()
                .map(|p| p.id_label().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    let user = UserConfig::load();
    let (_u, client) = market_client(None)?;
    let installer = Installer {
        project: &project,
        market: &client,
        user: &user,
    };
    let removed = installer.remove(candidates[0], args.no_deps)?;
    style.success(format!("已删除：{}", removed.join(", ")));
    Ok(())
}

fn plugin_outdated(market_url: Option<&str>, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let (_user, client) = market_client(market_url)?;
    let index = client.index(false)?;
    let locals = scan_plugins(&project.plugins_dir)?;
    let mut rows = Vec::new();
    for local in locals {
        let Some(id) = &local.id else { continue };
        let Some(remote) = index.plugins.iter().find(|p| &p.id == id) else {
            continue;
        };
        let Some(local_version) = local.version() else {
            continue;
        };
        if remote.parsed_version().is_newer_than(&local_version) {
            rows.push(vec![
                id.clone(),
                local.market_version.unwrap_or_default(),
                remote.version.clone(),
            ]);
        }
    }
    if style.json {
        return style.emit_json(&json!(rows));
    }
    table(style, &["插件 ID", "本地版本", "市场版本"], &rows);
    style.info(format!("{} 个插件可更新", rows.len()));
    Ok(())
}

fn info(style: &Style) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?;
    let project = Project::detect(&cwd);
    let mut checks = Vec::new();
    match project {
        Ok(p) => {
            checks.push(json!({"check":"project","ok":true,"kind":p.kind.label(),"root":p.root}));
            if p.kind == ProjectKind::BotInstance {
                checks.push(json!({"check":"config.json","ok":p.config_path.as_ref().map(|x|x.is_file()).unwrap_or(false)}));
                checks.push(
                    json!({"check":"plugins","ok":p.plugins_dir.is_dir(),"path":p.plugins_dir}),
                );
                checks.push(json!({"check":"venv","ok":p.venv_dir.is_some()}));
                checks.push(json!({"check":"branch","value":current_branch(&p.root)}));
            }
            if style.json {
                return style.emit_json(&json!(checks));
            }
            style.section("Jianer 项目体检");
            style.kv("类型", p.kind.label());
            style.kv("路径", p.root.display().to_string());
            if let Some(v) = &p.venv_dir {
                style.kv("虚拟环境", v.display().to_string());
            } else {
                style.warn("未找到 .venv/venv，插件 PyPI 依赖将无法隔离安装");
            }
            if let Some(branch) = current_branch(&p.root) {
                style.kv("Git 分支", branch);
            }
            if p.kind == ProjectKind::BotInstance {
                style.kv("插件数量", scan_plugins(&p.plugins_dir)?.len().to_string());
            }
        }
        Err(e) => {
            if style.json {
                return style.emit_json(&json!({"project":{"ok":false,"error":e.to_string()}}));
            }
            style.warn(e.to_string());
        }
    }
    Ok(())
}

fn update_conf(args: &UpdateConfArgs, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let mut user = UserConfig::load();
    if let Some(raw) = &args.channel {
        user.update.channel = Some(
            parse_channel(raw)
                .ok_or_else(|| CliError::Network("渠道只能是 main/NEXT-PREVIEW/dev".to_string()))?
                .branch()
                .to_string(),
        );
    }
    let channel = update::configure(
        &project,
        &mut user,
        args.non_interactive || args.channel.is_some(),
    )?;
    style.success(format!("已配置更新渠道：{}", channel.label()));
    Ok(())
}

fn upgrade(args: &UpgradeArgs, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let user = UserConfig::load();
    let channel = args
        .channel
        .as_deref()
        .and_then(parse_channel)
        .or_else(|| configured_channel(&project, &user))
        .ok_or(CliError::UpdateNotConfigured)?;
    update::upgrade(&project, channel, args.dry_run)?;
    style.success(format!("已从 {} 更新项目", channel.label()));
    Ok(())
}

fn run(args: &RunArgs, style: &Style) -> Result<()> {
    let project = Project::detect_bot(
        &std::env::current_dir().map_err(|e| CliError::io("读取当前目录", e))?,
    )?;
    let script = project
        .entry_script
        .clone()
        .ok_or_else(|| CliError::NotAProject {
            path: project.root.clone(),
            kind: "main.py 或 bot.py".to_string(),
        })?;
    let (python, _) = project.python_interpreter();
    if args.dry_run {
        println!(
            "{} {} {}",
            python.display(),
            script.display(),
            args.args.join(" ")
        );
        return Ok(());
    }
    style.info(format!("启动 {} …", script.display()));
    let status = Command::new(python)
        .arg(script.file_name().unwrap_or_default())
        .args(&args.args)
        .current_dir(&project.root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| CliError::PythonEnv(format!("启动 bot 失败：{e}")))?;
    if !status.success() {
        return Err(CliError::PythonEnv(format!(
            "bot 已退出，退出码 {:?}",
            status.code()
        )));
    }
    Ok(())
}
