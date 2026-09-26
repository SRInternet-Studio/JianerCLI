#![allow(dead_code)]

//! `index.json` 的数据模型。
//!
//! 字段与 `market-index.schema.json` 一一对应。schema 里所有带
//! `additionalProperties: false` 的对象在这里都用 `deny_unknown_fields`，
//! 这样上游加字段时本 CLI 会明确报错而不是静默忽略。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 智慧市场插件索引（`index.json`）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketIndex {
    /// 索引格式版本。本 CLI 只认识 [`crate::market::SUPPORTED_SCHEMA_VERSION`]。
    pub schema_version: u64,
    /// 索引生成时间（UTC，ISO 8601）。
    pub generated_at: String,
    /// 承载插件代码的 monorepo 信息。
    pub repository: Repository,
    /// 插件列表。
    #[serde(default)]
    pub plugins: Vec<MarketPlugin>,
}

/// 插件 monorepo 信息。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Repository {
    /// 仓库地址。
    pub url: String,
    /// 生成索引时的 commit SHA。
    pub r#ref: String,
    /// Release asset 下载模板，占位符 `{tag}` 与 `{asset}`。
    #[serde(default)]
    pub release_url_template: Option<String>,
}

impl Repository {
    /// 拼接某个插件的 zip 下载地址。
    ///
    /// 优先用索引里的模板；模板缺失时退回 GitHub 的约定路径，
    /// 这样即使生成端没写 `releaseUrlTemplate` 也能工作。
    pub fn asset_url(&self, tag: &str, asset: &str) -> String {
        let url = self
            .release_url_template
            .clone()
            .unwrap_or_else(|| format!("{}/releases/download/{{tag}}/{{asset}}", self.url));
        url.replace("{tag}", tag).replace("{asset}", asset)
    }

    /// 提取 `owner/repo`，用于调用 GitHub API。
    ///
    /// 支持 `https://github.com/owner/repo`、`git@github.com:owner/repo.git`
    /// 以及带代理前缀的地址（取最后两个路径段）。
    pub fn github_slug(&self) -> Option<String> {
        let cleaned = self
            .url
            .trim_end_matches(".git")
            .trim_end_matches('/')
            .replace(':', "/");
        let parts: Vec<&str> = cleaned.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return None;
        }
        Some(format!(
            "{}/{}",
            parts[parts.len() - 2],
            parts[parts.len() - 1]
        ))
    }
}

/// 一个插件在索引里的完整记录。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketPlugin {
    /// 插件唯一 ID，与 `__plugin_meta__.name` 一致。
    pub id: String,
    /// 展示名称。
    pub name: String,
    /// SemVer 版本号。
    pub version: String,
    /// 一句话描述。
    #[serde(default)]
    pub description: String,
    /// 使用说明，可含 `{reminder}` 占位符。
    #[serde(default)]
    pub usage: String,
    /// 插件形态。
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    /// 插件在仓库内的相对路径。
    pub path: String,
    /// 入口文件相对仓库根的路径。
    pub entry: String,
    /// 作者列表。
    #[serde(default)]
    pub authors: Vec<String>,
    /// 许可证。
    #[serde(default)]
    pub license: Option<String>,
    /// 项目主页。
    #[serde(default)]
    pub homepage: Option<String>,
    /// 搜索用标签。
    #[serde(default)]
    pub tags: Vec<String>,
    /// 依赖的其他市场插件 ID。
    #[serde(default)]
    pub requires: Vec<String>,
    /// Python 版本约束（PEP 440）。
    #[serde(default)]
    pub python_requires: Option<String>,
    /// 运行时 pip 依赖（PEP 508），安装后由 CLI 调 bot 的 venv 安装。
    #[serde(default)]
    pub pip_dependencies: Vec<String>,
    /// 兼容的 JianerCore 版本约束。
    #[serde(default)]
    pub core_requires: Option<String>,
    /// 当前版本的分发信息。
    pub release: ReleaseInfo,
    /// 插件文件最后变更时间。
    #[serde(default)]
    pub updated_at: Option<String>,
    /// 是否已弃用。
    #[serde(default)]
    pub deprecated: bool,
    /// 插件 README 的仓库相对路径。
    #[serde(default)]
    pub readme: Option<String>,
}

/// 插件形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// `plugins/` 下的单个 `.py` 文件。
    SingleFile,
    /// 含 `setup.py` 的插件目录。
    Package,
}

impl PluginType {
    /// 中文标签。
    pub fn label(self) -> &'static str {
        match self {
            PluginType::SingleFile => "单文件插件",
            PluginType::Package => "包插件",
        }
    }
}

/// 某个版本的分发信息。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseInfo {
    /// Release tag，形如 `<pluginId>@<version>`。
    pub tag: String,
    /// zip 文件名。
    pub asset: String,
    /// zip 字节数。
    pub size: u64,
    /// zip 的 SHA-256，下载后必须校验。
    pub sha256: String,
}

impl MarketPlugin {
    /// 把版本号按 SemVer 拆成可比较的三元组 + 预发布标识。
    ///
    /// 只实现比较所需的部分语义：`1.2.3` > `1.2.3-beta`，数字段按数值比较。
    pub fn parsed_version(&self) -> SemVer {
        SemVer::parse(&self.version)
    }

    /// 该插件是否命中搜索关键词。
    ///
    /// 与 schema 的说明一致：匹配 `id` / `name` / `description` / `tags`。
    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.id.to_lowercase().contains(&q)
            || self.name.to_lowercase().contains(&q)
            || self.description.to_lowercase().contains(&q)
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }

    /// 按 ID 或名称做模糊匹配，返回匹配强度（越小越精确），供排序。
    pub fn fuzzy_score(&self, query: &str) -> Option<usize> {
        let q = query.to_lowercase();
        let id = self.id.to_lowercase();
        let name = self.name.to_lowercase();
        if id == q || name == q {
            return Some(0);
        }
        // ID 去掉市场前缀后的精确匹配，例如 "advanced-quote" 命中
        // "jianerbot-plugin-advanced-quote"。
        let short = id
            .strip_prefix(crate::local::PLUGIN_ID_PREFIX)
            .unwrap_or(&id);
        if short == q {
            return Some(1);
        }
        if short.ends_with(&format!("-{q}")) {
            return Some(2);
        }
        if id.contains(&q) || name.contains(&q) {
            return Some(3);
        }
        None
    }
}

/// 索引里所有插件的 `id -> plugin` 视图。
pub fn by_id(index: &MarketIndex) -> BTreeMap<&str, &MarketPlugin> {
    index.plugins.iter().map(|p| (p.id.as_str(), p)).collect()
}

/// 极简 SemVer，只用于版本比较与「是否有更新」判断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    /// 主版本、次版本、修订号。
    pub numbers: (u64, u64, u64),
    /// 预发布标识，空表示正式版。
    pub pre: String,
}

impl SemVer {
    /// 解析版本串；无法解析时回退为全零，使比较退化成「无更新」。
    pub fn parse(raw: &str) -> SemVer {
        let (core, pre) = match raw.split_once('-') {
            Some((c, p)) => (c, p.to_string()),
            None => (raw, String::new()),
        };
        let mut it = core.split('.').map(|s| s.parse::<u64>().unwrap_or(0));
        SemVer {
            numbers: (
                it.next().unwrap_or(0),
                it.next().unwrap_or(0),
                it.next().unwrap_or(0),
            ),
            pre,
        }
    }

    /// `self` 是否比 `other` 新。
    ///
    /// 预发布版本视为比同号正式版旧：`1.0.0-beta` < `1.0.0`。
    pub fn is_newer_than(&self, other: &SemVer) -> bool {
        if self.numbers != other.numbers {
            return self.numbers > other.numbers;
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, false) => true,
            (false, true) => false,
            _ => self.pre > other.pre,
        }
    }
}
