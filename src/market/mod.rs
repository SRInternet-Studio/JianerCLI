//! 智慧市场：索引模型、来源解析、缓存与下载。
//!
//! 智慧市场是**纯静态**服务：插件索引 `index.json` 放在插件 monorepo 里，
//! 每个 `pluginId@version` 的 zip 挂在 GitHub Release 上。CLI 做三件事：
//!
//! 1. 拉取并解析 `index.json`（[`MarketIndex`]），本地缓存一段时间；
//! 2. 按 `releaseUrlTemplate` 拼接 zip 下载地址并下载（[`MarketSource`]）；
//! 3. 用 GitHub API 查 asset 的 `download_count`，供 `plugin show --r` 展示。
//!
//! 索引格式由 `market-index.schema.json` 定义，本模块的 serde 结构与之一一对应。

pub mod client;
pub mod index;

pub use client::{DownloadedAsset, MarketClient, MarketSource};
pub use index::{MarketIndex, MarketPlugin, Repository};

/// 本 CLI 支持的索引 schema 版本。
pub const SUPPORTED_SCHEMA_VERSION: u64 = 1;

/// 内置的默认市场索引地址。
///
/// 市场 monorepo 尚未建立（`SR-Internet/jianer-market` 目前 404），
/// 因此这里给出的是约定的默认值，可通过用户配置或环境变量覆盖。
pub const DEFAULT_MARKET_INDEX_URL: &str =
    "https://raw.githubusercontent.com/SR-Internet/jianer-market/main/index.json";
