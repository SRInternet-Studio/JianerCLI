//! 本地 Jianer 项目探测。
//!
//! 方案文档要求：只有当当前项目是 `Jianer_QQ_bot` 实例时，才开放 `config`
//! 与 `plugin` 等特殊功能。所以每个子命令启动时都要先回答两个问题：
//!
//! 1. 当前目录是不是一个 Jianer 项目？是 Core 项目还是 bot 实例？
//! 2. 如果是 bot 实例，它的 `plugins/` 目录和 Python 解释器在哪？
//!
//! 判定依据刻意选得保守（只看结构性特征，不猜）：
//!
//! - **bot 实例**：存在 `config.json` 且其 JSON 里含 `connections` 或
//!   `others` 键，同时存在 `plugins/` 目录或 `main.py`。
//! - **Core 项目**：存在 `pyproject.toml`/`setup.py` 且能定位到
//!   `jianer/__init__.py`，或目录树里直接有 `jianer/__init__.py`。
//!
//! 向上逐级查找，允许用户在项目的子目录里执行命令。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{CliError, IoContext, Result};

/// 项目种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKind {
    /// 基于 JianerCore 的 bot 实例（例如 Jianer_QQ_bot）。
    BotInstance,
    /// JianerCore 框架自身。
    Core,
}

impl ProjectKind {
    /// 面向用户的中文描述。
    pub fn label(self) -> &'static str {
        match self {
            ProjectKind::BotInstance => "Jianer_QQ_bot 实例",
            ProjectKind::Core => "JianerCore 项目",
        }
    }
}

/// 探测到的项目信息。
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    /// 项目根目录。
    pub root: PathBuf,
    /// 项目种类。
    pub kind: ProjectKind,
    /// 插件目录（bot 实例才有意义，Core 项目也可能有）。
    pub plugins_dir: PathBuf,
    /// `config.json` 路径（仅 bot 实例）。
    pub config_path: Option<PathBuf>,
    /// bot 主入口脚本（仅 bot 实例）。
    pub entry_script: Option<PathBuf>,
    /// 项目内 `.venv` 目录（若存在）。
    pub venv_dir: Option<PathBuf>,
    /// bot 实例的展示名，取自 config.json 的 `others.bot_name`。
    pub display_name: Option<String>,
}

impl Project {
    /// 探测当前目录所属的项目。`start` 通常是 `std::env::current_dir()`。
    pub fn detect(start: &Path) -> Result<Project> {
        let mut dir = Some(start);
        while let Some(current) = dir {
            if let Some(project) = Self::probe(current)? {
                return Ok(project);
            }
            dir = current.parent();
        }
        Err(CliError::NotAProject {
            path: start.to_path_buf(),
            kind: "JianerCore 项目或 Jianer_QQ_bot 实例".to_string(),
        })
    }

    /// 探测当前目录所属的 bot 实例；如果不是 bot 实例则报错。
    ///
    /// `config` 与 `plugin` 命令都要求这个前置条件。
    pub fn detect_bot(start: &Path) -> Result<Project> {
        let project = Self::detect(start)?;
        if project.kind != ProjectKind::BotInstance {
            return Err(CliError::BotFeatureUnavailable);
        }
        Ok(project)
    }

    /// 检查单个目录是否构成一个项目。不匹配返回 `Ok(None)`。
    fn probe(dir: &Path) -> Result<Option<Project>> {
        let config_path = dir.join("config.json");
        let plugins_dir = dir.join("plugins");

        // bot 实例判定：config.json 的结构 + 目录布局特征。
        if config_path.is_file() {
            let looks_like_bot_config = Self::config_has_bot_shape(&config_path);
            let has_bot_layout = plugins_dir.is_dir()
                || dir.join("main.py").is_file()
                || dir.join("bot.py").is_file();
            if looks_like_bot_config && has_bot_layout {
                return Ok(Some(Self::assemble_bot(dir, config_path, plugins_dir)?));
            }
        }

        // Core 项目判定：能找到 jianer 包。
        let core_pkg = dir.join("jianer").join("__init__.py");
        if core_pkg.is_file() {
            let mut project = Self::assemble_core(dir);
            project.plugins_dir = plugins_dir;
            return Ok(Some(project));
        }

        Ok(None)
    }

    /// 判断 config.json 是否具备 Jianer bot 配置的结构特征。
    ///
    /// 只在 JSON 合法且是对象时才算命中，避免把任意同名文件误判为 bot 配置。
    fn config_has_bot_shape(path: &Path) -> bool {
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return false;
        };
        let Some(obj) = value.as_object() else {
            return false;
        };
        obj.contains_key("connections")
            || obj.contains_key("Connections")
            || obj.contains_key("others")
            || obj.contains_key("protocol")
    }

    /// 组装 bot 实例信息。
    fn assemble_bot(dir: &Path, config_path: PathBuf, plugins_dir: PathBuf) -> Result<Project> {
        let entry_script = ["main.py", "bot.py"]
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file());

        let display_name = read_config_value(&config_path).ok().and_then(|v| {
            v.get("others")
                .and_then(|o| o.get("bot_name"))
                .and_then(|n| n.as_str())
                .map(str::to_string)
        });

        Ok(Project {
            root: dir.to_path_buf(),
            kind: ProjectKind::BotInstance,
            plugins_dir,
            config_path: Some(config_path),
            entry_script,
            venv_dir: detect_venv(dir),
            display_name,
        })
    }

    /// 组装 Core 项目信息。
    fn assemble_core(dir: &Path) -> Project {
        Project {
            root: dir.to_path_buf(),
            kind: ProjectKind::Core,
            plugins_dir: dir.join("plugins"),
            config_path: None,
            entry_script: None,
            venv_dir: detect_venv(dir),
            display_name: None,
        }
    }

    /// 项目根目录下的 `config.json` 路径，缺失时报错。
    pub fn config_path(&self) -> Result<&Path> {
        self.config_path
            .as_deref()
            .ok_or_else(|| CliError::NotAProject {
                path: self.root.clone(),
                kind: "config.json".to_string(),
            })
    }

    /// 解析出用于执行 pip / python 的解释器路径。
    ///
    /// 优先级：项目内 `.venv` → 环境变量 `JIANER_PYTHON` → `python3`/`python`。
    /// 返回 `(解释器路径, 是否是项目 venv)`。
    pub fn python_interpreter(&self) -> (PathBuf, bool) {
        if let Some(venv) = &self.venv_dir {
            for candidate in interpreter_candidates(venv) {
                if candidate.is_file() {
                    return (candidate, true);
                }
            }
        }
        if let Some(explicit) = std::env::var_os("JIANER_PYTHON") {
            return (PathBuf::from(explicit), false);
        }
        for name in ["python3", "python"] {
            if let Some(found) = which(name) {
                return (found, false);
            }
        }
        (PathBuf::from("python3"), false)
    }
}

/// 读取并解析 `config.json`。
pub fn read_config_value(path: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(path).ctx(format!("读取配置文件 {}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|e| CliError::Network(format!("解析 {} 失败：{e}", path.display())))
}

/// 探测项目根目录下的虚拟环境。
fn detect_venv(dir: &Path) -> Option<PathBuf> {
    for name in [".venv", "venv", "env"] {
        let candidate = dir.join(name);
        if candidate.join("pyvenv.cfg").is_file() || candidate.join("bin").is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// 列出虚拟环境下解释器的候选路径（覆盖 Windows 与类 Unix）。
pub fn interpreter_candidates(venv: &Path) -> Vec<PathBuf> {
    vec![
        venv.join("bin").join("python3"),
        venv.join("bin").join("python"),
        venv.join("Scripts").join("python.exe"),
    ]
}

/// 在 `PATH` 里查找可执行文件。
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}
