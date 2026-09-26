#![allow(dead_code)]

//! 插件安装、升级、卸载，以及 PyPI 运行时依赖安装。

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::UserConfig;
use crate::error::{CliError, IoContext, Result};
use crate::local::{scan_plugins, LocalPlugin};
use crate::market::{DownloadedAsset, MarketClient, MarketPlugin};
use crate::project::Project;

/// 插件安装器。
pub struct Installer<'a> {
    /// 当前 bot 项目。
    pub project: &'a Project,
    /// 市场客户端。
    pub market: &'a MarketClient,
    /// 用户配置。
    pub user: &'a UserConfig,
}

impl<'a> Installer<'a> {
    /// 按市场记录安装一个插件；返回实际安装的插件 ID。
    pub fn install(&self, plugin: &MarketPlugin, upgrade: bool) -> Result<String> {
        if plugin.deprecated {
            eprintln!("⚠ 插件 {} 已标记为弃用，仍继续安装。", plugin.id);
        }
        let asset = self.market.download_asset(
            &self.market_index_repository(),
            &plugin.release.tag,
            &plugin.release.asset,
            &plugin.release.sha256,
        )?;
        self.install_asset(plugin, &asset, upgrade)?;
        Ok(plugin.id.clone())
    }

    /// 安装已下载的包。
    pub fn install_asset(
        &self,
        plugin: &MarketPlugin,
        asset: &DownloadedAsset,
        _upgrade: bool,
    ) -> Result<()> {
        let plugins_dir = &self.project.plugins_dir;
        std::fs::create_dir_all(plugins_dir)
            .ctx(format!("创建插件目录 {}", plugins_dir.display()))?;

        let staging = plugins_dir.join(format!(".jianer-install-{}", plugin.id));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)
                .ctx(format!("清理旧安装暂存目录 {}", staging.display()))?;
        }
        std::fs::create_dir_all(&staging).ctx(format!("创建安装暂存目录 {}", staging.display()))?;
        if let Err(e) = extract_zip_safe(&asset.path, &staging) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }

        // zip 约定根为插件目录名/文件名；把暂存目录的内容合并到 plugins/。
        let roots = immediate_entries(&staging)?;
        if roots.is_empty() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(CliError::Network(format!(
                "插件包 {} 为空",
                plugin.release.asset
            )));
        }
        let mut installed_root: Option<PathBuf> = None;
        for root in roots {
            let target = plugins_dir.join(root.file_name().unwrap_or_default());
            if target.exists() {
                backup_path(&target)?;
                // backup_path 已将目标移到 .bak，目标路径此时不存在。
            }
            std::fs::rename(&root, &target).ctx(format!("安装插件文件到 {}", target.display()))?;
            installed_root = Some(target);
        }
        let _ = std::fs::remove_dir_all(&staging);

        // 把市场版本写入 sidecar，供 plugin list/outdated 使用。
        if let Some(root) = installed_root {
            let sidecar = if root.is_dir() {
                root.join("market.json")
            } else {
                root.with_file_name(format!(
                    "{}.market.json",
                    root.file_stem().unwrap_or_default().to_string_lossy()
                ))
            };
            let market = serde_json::json!({
                "id": plugin.id,
                "version": plugin.version,
                "source": "jianer-market",
                "release": plugin.release.tag,
            });
            let text = serde_json::to_string_pretty(&market)
                .map_err(|e| CliError::Network(format!("序列化插件市场记录失败：{e}")))?;
            std::fs::write(&sidecar, format!("{text}\n"))
                .ctx(format!("写入插件版本记录 {}", sidecar.display()))?;
        }
        Ok(())
    }

    /// 安装完插件后，调用项目 venv 的 pip 安装其 PyPI 依赖。
    ///
    /// 这是插件安装闭环的一部分：插件本体不是 pip 包，只有它声明的
    /// `pipDependencies` 才交给 pip。命令使用 `python -m pip`，避免 PATH
    /// 上的 pip 与项目解释器不一致。
    pub fn install_pip_dependencies(&self, plugin: &MarketPlugin, dry_run: bool) -> Result<()> {
        if plugin.pip_dependencies.is_empty() || !self.user.install.auto_pip {
            return Ok(());
        }
        let (python, is_venv) = self.project.python_interpreter();
        if !is_venv {
            eprintln!(
                "⚠ 项目没有检测到 .venv，将使用 {}；如需隔离环境请先创建 .venv。",
                python.display()
            );
        }
        let mut args = vec!["-m".to_string(), "pip".to_string(), "install".to_string()];
        if self.user.install.pip_upgrade {
            args.push("--upgrade".to_string());
        }
        args.extend(plugin.pip_dependencies.iter().cloned());
        println!(
            "正在为 {} 安装 PyPI 依赖：{}",
            plugin.id,
            plugin.pip_dependencies.join(", ")
        );
        if dry_run {
            println!("  {} {}", python.display(), args.join(" "));
            return Ok(());
        }
        let status = Command::new(&python)
            .args(&args)
            .current_dir(&self.project.root)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| CliError::PythonEnv(format!("启动 {} 失败：{e}", python.display())))?;
        if !status.success() {
            return Err(CliError::PythonEnv(format!(
                "插件 {} 的 PyPI 依赖安装失败（退出码 {:?}）",
                plugin.id,
                status.code()
            )));
        }
        Ok(())
    }

    /// 删除本地插件；默认同时删除仅被它依赖且无其他插件使用的依赖。
    pub fn remove(&self, plugin: &LocalPlugin, no_deps: bool) -> Result<Vec<String>> {
        let all = scan_plugins(&self.project.plugins_dir)?;
        let mut removed = Vec::new();
        remove_path(&plugin.path)?;
        removed.push(plugin.id_label().to_string());
        if !no_deps {
            if let Some(meta) = &plugin.meta {
                let installed_ids: BTreeSet<String> = all
                    .iter()
                    .filter_map(|p| p.id.clone())
                    .filter(|id| id != &meta.name)
                    .collect();
                for dep in &meta.requires {
                    if crate::local::BUILTIN_PLUGIN_IDS.contains(&dep.as_str()) {
                        continue;
                    }
                    let still_needed = all.iter().any(|other| {
                        other.id.as_deref() != Some(&meta.name)
                            && other
                                .meta
                                .as_ref()
                                .map(|m| m.requires.iter().any(|r| r == dep))
                                .unwrap_or(false)
                    });
                    if !still_needed && installed_ids.contains(dep) {
                        if let Some(dep_plugin) = all.iter().find(|p| p.id.as_deref() == Some(dep))
                        {
                            remove_path(&dep_plugin.path)?;
                            removed.push(dep.clone());
                        }
                    }
                }
            }
        }
        Ok(removed)
    }

    /// 为安装器设置当前索引的 repository；避免把它塞进每个 MarketPlugin。
    pub fn with_repository(self, repository: &'a crate::market::Repository) -> BoundInstaller<'a> {
        BoundInstaller {
            inner: self,
            repository,
        }
    }
}

/// 带 repository 的安装器，实际执行下载。
pub struct BoundInstaller<'a> {
    inner: Installer<'a>,
    repository: &'a crate::market::Repository,
}

impl<'a> BoundInstaller<'a> {
    /// 下载、解压、写版本记录、安装 PyPI 依赖。
    pub fn install(&self, plugin: &MarketPlugin, upgrade: bool, dry_run: bool) -> Result<()> {
        let installed = scan_plugins(&self.inner.project.plugins_dir)?;
        if !upgrade
            && installed
                .iter()
                .any(|p| p.id.as_deref() == Some(&plugin.id))
        {
            return Err(CliError::Network(format!(
                "插件 {} 已安装；如需升级请加 -U，或用 --all 强制重装",
                plugin.id
            )));
        }
        if dry_run {
            println!(
                "将安装 {} {}（{}）",
                plugin.id, plugin.version, plugin.release.asset
            );
            if !plugin.pip_dependencies.is_empty() {
                println!("  PyPI 依赖：{}", plugin.pip_dependencies.join(", "));
            }
            return Ok(());
        }
        let asset = self.inner.market.download_asset(
            self.repository,
            &plugin.release.tag,
            &plugin.release.asset,
            &plugin.release.sha256,
        )?;
        self.inner.install_asset(plugin, &asset, upgrade)?;
        self.inner.install_pip_dependencies(plugin, false)?;
        Ok(())
    }
}

fn extract_zip_safe(zip_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(zip_path).ctx(format!("打开插件包 {}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| CliError::Network(format!("读取插件 zip 失败：{e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CliError::Network(format!("读取插件 zip 条目失败：{e}")))?;
        let Some(enclosed) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err(CliError::Network(format!(
                "插件包包含不安全路径：{}",
                entry.name()
            )));
        };
        let output = destination.join(enclosed);
        if entry.is_dir() {
            std::fs::create_dir_all(&output).ctx(format!("创建 zip 目录 {}", output.display()))?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .ctx(format!("创建 zip 父目录 {}", parent.display()))?;
            }
            let mut out =
                File::create(&output).ctx(format!("创建 zip 文件 {}", output.display()))?;
            std::io::copy(&mut entry, &mut out)
                .ctx(format!("解压 zip 文件 {}", output.display()))?;
        }
    }
    Ok(())
}

fn immediate_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::read_dir(dir)
        .ctx(format!("读取安装暂存目录 {}", dir.display()))?
        .map(|entry| entry.map(|e| e.path()).ctx("读取暂存条目"))
        .collect()
}

fn backup_path(path: &Path) -> Result<()> {
    let backup = path.with_extension(format!(
        "{}bak",
        path.extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    if backup.exists() {
        remove_path(&backup)?;
    }
    std::fs::rename(path, backup).ctx(format!("备份插件 {}", path.display()))
}

fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).ctx(format!("删除目录 {}", path.display()))
    } else {
        std::fs::remove_file(path).ctx(format!("删除文件 {}", path.display()))
    }
}

// Placeholder kept out of public API: the bound installer carries the repository.
impl<'a> Installer<'a> {
    fn market_index_repository(&self) -> crate::market::Repository {
        crate::market::Repository {
            url: String::new(),
            r#ref: String::new(),
            release_url_template: None,
        }
    }
}
