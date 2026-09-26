#![allow(dead_code)]

//! 配置：用户级配置（`~/.config/jianer-cli/config.toml`）与 bot 配置编辑。
//!
//! 两类配置刻意分开：
//!
//! - **用户级配置**由 JianerCLI 自己拥有（市场地址、下载并发等），存 TOML；
//! - **bot 配置**是 `config.json`，属于 bot 项目，CLI 只做「读—改—写回」，
//!   且必须保留未知字段（bot 与插件都可能往里塞自定义键）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, IoContext, Result};

/// 用户级配置的根。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UserConfig {
    /// 智慧市场相关。
    pub market: MarketConfig,
    /// 插件安装相关。
    pub install: InstallConfig,
    /// 更新渠道相关。
    pub update: UpdateConfig,
}

/// 市场配置。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MarketConfig {
    /// 索引地址；留空则用内置默认值。
    pub index_url: Option<String>,
}

/// 安装行为配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct InstallConfig {
    /// 插件装好后是否自动用 bot 的 venv 安装 `pipDependencies`。
    pub auto_pip: bool,
    /// 自动 pip 安装时是否加 `--upgrade`。
    pub pip_upgrade: bool,
}

impl Default for InstallConfig {
    fn default() -> Self {
        // 默认开启自动安装：方案文档要求「下载完整后自动调用 bot 的 venv 安装」。
        InstallConfig {
            auto_pip: true,
            pip_upgrade: false,
        }
    }
}

/// 更新渠道配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateConfig {
    /// 默认使用哪个渠道（`main` / `NEXT-PREVIEW` / `dev`）。
    pub channel: Option<String>,
    /// 更新前是否自动备份。
    pub backup: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        UpdateConfig {
            channel: None,
            backup: true,
        }
    }
}

impl UserConfig {
    /// 用户配置文件路径：`$XDG_CONFIG_HOME/jianer-cli/config.toml`。
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("jianer-cli").join("config.toml"))
    }

    /// 读取用户配置；不存在或损坏时返回默认值（不阻断命令）。
    pub fn load() -> UserConfig {
        let Some(path) = Self::path() else {
            return UserConfig::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return UserConfig::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// 写回用户配置。
    pub fn save(&self) -> Result<PathBuf> {
        let path =
            Self::path().ok_or_else(|| CliError::PythonEnv("无法确定用户配置目录".to_string()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ctx(format!("创建配置目录 {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| CliError::Network(format!("序列化用户配置失败：{e}")))?;
        std::fs::write(&path, text).ctx(format!("写入用户配置 {}", path.display()))?;
        Ok(path)
    }

    /// 缓存目录：`$XDG_CACHE_HOME/jianer-cli`。
    pub fn cache_dir() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("jianer-cli")
    }
}

/// bot 的 `config.json` 编辑器：保留未知字段，只改动目标键。
pub struct BotConfigFile {
    path: PathBuf,
    value: serde_json::Value,
}

impl BotConfigFile {
    /// 载入 `config.json`。
    pub fn load(path: &Path) -> Result<BotConfigFile> {
        let value = crate::project::read_config_value(path)?;
        Ok(BotConfigFile {
            path: path.to_path_buf(),
            value,
        })
    }

    /// 只读视图。
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }

    /// 取得某个顶层键的字符串值。
    pub fn get_str(&self, key: &str) -> Option<String> {
        self.value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// 取得某个顶层键。
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.value.get(key)
    }

    /// 设置顶层字符串键。
    pub fn set_str(&mut self, key: &str, value: &str) -> Result<()> {
        let obj = self.root_object_mut()?;
        obj.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
        Ok(())
    }

    /// 设置 `others.<key>`。
    pub fn set_other_str(&mut self, key: &str, value: &str) -> Result<()> {
        self.others_mut()?.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
        Ok(())
    }

    /// 设置 `others.<key>` 为整数。
    pub fn set_other_u64(&mut self, key: &str, value: u64) -> Result<()> {
        self.others_mut()?
            .insert(key.to_string(), serde_json::Value::Number(value.into()));
        Ok(())
    }

    /// 设置 `others.<key>` 为布尔。
    pub fn set_other_bool(&mut self, key: &str, value: bool) -> Result<()> {
        self.others_mut()?
            .insert(key.to_string(), serde_json::Value::Bool(value));
        Ok(())
    }

    /// 设置 `others.<key>` 为字符串数组。
    pub fn set_other_string_list(&mut self, key: &str, values: &[String]) -> Result<()> {
        let arr = values
            .iter()
            .map(|v| serde_json::Value::String(v.clone()))
            .collect();
        self.others_mut()?
            .insert(key.to_string(), serde_json::Value::Array(arr));
        Ok(())
    }

    /// 设置顶层字符串数组（`owner` / `black_list` / `silents`）。
    pub fn set_string_list(&mut self, key: &str, values: &[String]) -> Result<()> {
        let arr = values
            .iter()
            .map(|v| serde_json::Value::String(v.clone()))
            .collect();
        self.root_object_mut()?
            .insert(key.to_string(), serde_json::Value::Array(arr));
        Ok(())
    }

    /// 设置某个连接（`connections.<protocol>.<key>`）。
    pub fn set_connection(
        &mut self,
        protocol: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        let Some(connections) = self
            .value
            .get_mut("connections")
            .and_then(|c| c.as_object_mut())
        else {
            return Err(CliError::Network(
                "config.json 中缺少 connections 段".to_string(),
            ));
        };
        // 协议名做规范化匹配，避免大小写差异导致新建重复段。
        let existing_key = connections
            .keys()
            .find(|k| k.eq_ignore_ascii_case(protocol))
            .cloned()
            .unwrap_or_else(|| protocol.to_string());
        let entry = connections
            .entry(existing_key)
            .or_insert_with(|| serde_json::json!({}));
        let Some(obj) = entry.as_object_mut() else {
            return Err(CliError::Network(format!(
                "config.json 的 connections.{protocol} 不是对象"
            )));
        };
        obj.insert(key.to_string(), value);
        Ok(())
    }

    /// 读取某个连接的所有键值。
    pub fn connection(
        &self,
        protocol: &str,
    ) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.value
            .get("connections")?
            .as_object()?
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(protocol))
            .and_then(|(_, v)| v.as_object())
    }

    /// 列出所有已配置的协议名。
    pub fn connection_names(&self) -> Vec<String> {
        self.value
            .get("connections")
            .and_then(|c| c.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 写回磁盘（先备份，再原子替换）。
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ctx(format!("创建配置目录 {}", parent.display()))?;
        }
        // 备份原文件，便于用户手滑后恢复。
        if self.path.is_file() {
            let backup = self.path.with_extension("json.bak");
            let _ = std::fs::copy(&self.path, &backup);
        }
        let text = serde_json::to_string_pretty(&self.value)
            .map_err(|e| CliError::Network(format!("序列化 config.json 失败：{e}")))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, format!("{text}\n")).ctx(format!("写入 {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).ctx(format!("替换 {}", self.path.display()))?;
        Ok(())
    }

    fn root_object_mut(&mut self) -> Result<&mut serde_json::Map<String, serde_json::Value>> {
        self.value
            .as_object_mut()
            .ok_or_else(|| CliError::Network("config.json 顶层不是对象".to_string()))
    }

    fn others_mut(&mut self) -> Result<&mut serde_json::Map<String, serde_json::Value>> {
        let root = self.root_object_mut()?;
        let entry = root
            .entry("others".to_string())
            .or_insert_with(|| serde_json::json!({}));
        entry
            .as_object_mut()
            .ok_or_else(|| CliError::Network("config.json 的 others 段不是对象".to_string()))
    }
}

/// 把逗号/空格分隔的用户输入拆成列表。
///
/// 支持用户在向导里一次输入多个 ID，例如 `123, 456 789`。
pub fn split_list(input: &str) -> Vec<String> {
    input
        .split([',', '，', ' ', '\t', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// 把列表合并成用户可编辑的单行文本。
pub fn join_list(values: &[String]) -> String {
    values.join(", ")
}
