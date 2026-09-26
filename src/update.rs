//! 更新渠道检测与 bot 源码升级。

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use inquire::{Confirm, Select};

use crate::config::{BotConfigFile, UserConfig};
use crate::error::{CliError, Result};
use crate::project::Project;

/// 官方支持的更新渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Main,
    NextPreview,
    Dev,
}

impl Channel {
    pub fn all() -> Vec<Channel> {
        vec![Channel::Main, Channel::NextPreview, Channel::Dev]
    }
    pub fn branch(self) -> &'static str {
        match self {
            Channel::Main => "main",
            Channel::NextPreview => "NEXT-PREVIEW",
            Channel::Dev => "dev",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Channel::Main => "main (Fixed Release)",
            Channel::NextPreview => "NEXT-PREVIEW (Curated Rolling Release)",
            Channel::Dev => "dev (Rolling Release)",
        }
    }
    pub fn warning(self) -> Option<&'static str> {
        match self {
            Channel::Dev => Some("dev 分支更新频率最高且最不稳定，不建议用于生产环境。选择后必须完整阅读 10 秒免责声明。"),
            _ => None,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// 从字符串解析官方渠道。
pub fn parse_channel(value: &str) -> Option<Channel> {
    match value.to_ascii_lowercase().as_str() {
        "main" => Some(Channel::Main),
        "next-preview" | "next_preview" | "nextpreview" => Some(Channel::NextPreview),
        "dev" => Some(Channel::Dev),
        _ => None,
    }
}

/// 运行 update-conf 交互向导。
pub fn configure(
    project: &Project,
    user: &mut UserConfig,
    non_interactive: bool,
) -> Result<Channel> {
    let current = user.update.channel.as_deref().and_then(parse_channel);
    let channel = if let Some(ch) = current {
        if non_interactive {
            ch
        } else {
            Select::new("选择 Jianer_QQ_bot 更新渠道", Channel::all())
                .prompt()
                .map_err(|_| CliError::Cancelled)?
        }
    } else if non_interactive {
        Channel::Main
    } else {
        Select::new("选择 Jianer_QQ_bot 更新渠道", Channel::all())
            .prompt()
            .map_err(|_| CliError::Cancelled)?
    };
    if let Some(warning) = channel.warning() {
        println!("⚠ {warning}");
        if !non_interactive {
            println!("请阅读以上说明。10 秒后才能继续。");
            std::thread::sleep(Duration::from_secs(10));
            let ok = Confirm::new("我已了解 dev 分支风险，继续配置？")
                .with_default(false)
                .prompt()
                .map_err(|_| CliError::Cancelled)?;
            if !ok {
                return Err(CliError::Cancelled);
            }
        }
    }
    user.update.channel = Some(channel.branch().to_string());
    user.save()?;
    // 同时把渠道写进项目自己的配置，便于项目迁移。
    if let Some(config_path) = &project.config_path {
        let mut config = BotConfigFile::load(config_path)?;
        config.set_str("jianer_cli_update_channel", channel.branch())?;
        config.save()?;
    }
    Ok(channel)
}

/// 从用户配置/项目配置解析当前渠道。
pub fn configured_channel(project: &Project, user: &UserConfig) -> Option<Channel> {
    user.update
        .channel
        .as_deref()
        .and_then(parse_channel)
        .or_else(|| {
            project
                .config_path
                .as_deref()
                .and_then(|p| BotConfigFile::load(p).ok())
                .and_then(|c| c.get_str("jianer_cli_update_channel"))
                .and_then(|s| parse_channel(&s))
        })
}

/// 从远程 Git 获取指定渠道的最新代码。
pub fn upgrade(project: &Project, channel: Channel, dry_run: bool) -> Result<()> {
    if !project.root.join(".git").exists() {
        return Err(CliError::Network(format!(
            "项目 {} 不是 Git 仓库",
            project.root.display()
        )));
    }
    let branch = channel.branch();
    if dry_run {
        println!(
            "将执行：git fetch origin {branch} && git checkout {branch} && git pull --ff-only"
        );
        return Ok(());
    }
    let fetch = Command::new("git")
        .args(["fetch", "origin", branch])
        .current_dir(&project.root)
        .status()
        .map_err(|e| CliError::Network(format!("执行 git fetch 失败：{e}")))?;
    if !fetch.success() {
        return Err(CliError::Network("git fetch 失败".to_string()));
    }
    let checkout = Command::new("git")
        .args(["checkout", branch])
        .current_dir(&project.root)
        .status()
        .map_err(|e| CliError::Network(format!("执行 git checkout 失败：{e}")))?;
    if !checkout.success() {
        return Err(CliError::Network("git checkout 失败".to_string()));
    }
    let pull = Command::new("git")
        .args(["pull", "--ff-only", "origin", branch])
        .current_dir(&project.root)
        .status()
        .map_err(|e| CliError::Network(format!("执行 git pull 失败：{e}")))?;
    if !pull.success() {
        return Err(CliError::Network(
            "git pull 失败（可能有本地修改或无法快进）".to_string(),
        ));
    }
    Ok(())
}

/// 返回仓库当前分支。
pub fn current_branch(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
