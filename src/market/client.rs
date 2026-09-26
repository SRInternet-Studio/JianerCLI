#![allow(dead_code)]

//! 市场索引的获取、缓存，以及插件包的下载与校验。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::config::UserConfig;
use crate::error::{CliError, IoContext, NetContext, Result};
use crate::market::index::{MarketIndex, Repository};
use crate::market::SUPPORTED_SCHEMA_VERSION;

/// 索引缓存有效期：10 分钟，避免同一条命令里反复联网。
const INDEX_TTL: Duration = Duration::from_secs(600);

/// 市场来源：从「索引地址」推导出「仓库信息」，从而能拼接下载 URL。
#[derive(Debug, Clone)]
pub struct MarketSource {
    /// 索引 JSON 的 URL。
    pub index_url: String,
}

impl MarketSource {
    /// 按「命令行参数 → 环境变量 → 用户配置 → 内置默认值」的优先级解析来源。
    pub fn resolve(cli_override: Option<&str>, user: &UserConfig) -> MarketSource {
        let index_url = cli_override
            .map(str::to_string)
            .or_else(|| std::env::var("JIANER_MARKET_URL").ok())
            .or_else(|| user.market.index_url.clone())
            .unwrap_or_else(|| crate::market::DEFAULT_MARKET_INDEX_URL.to_string());
        MarketSource { index_url }
    }
}

/// 下载完成的插件包。
pub struct DownloadedAsset {
    /// 落地到磁盘的临时文件路径。
    pub path: PathBuf,
    /// 实际字节数。
    pub size: u64,
    /// 实际 SHA-256（十六进制小写）。
    pub sha256: String,
}

/// 市场客户端：持有 HTTP 连接与缓存目录。
pub struct MarketClient {
    http: reqwest::blocking::Client,
    source: MarketSource,
    cache_dir: PathBuf,
}

impl MarketClient {
    /// 创建客户端。`cache_dir` 一般是 `~/.cache/jianer-cli`。
    pub fn new(source: MarketSource, cache_dir: PathBuf) -> Result<MarketClient> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("jianer-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .net("初始化 HTTP 客户端")?;
        std::fs::create_dir_all(&cache_dir).ctx(format!("创建缓存目录 {}", cache_dir.display()))?;
        Ok(MarketClient {
            http,
            source,
            cache_dir,
        })
    }

    /// 索引地址。
    pub fn index_url(&self) -> &str {
        &self.source.index_url
    }

    /// 拉取索引，优先用本地缓存。
    ///
    /// `force_refresh` 为 true 时忽略缓存（`--refresh`）。
    pub fn index(&self, force_refresh: bool) -> Result<MarketIndex> {
        let cached = self.cached_index_path();
        if !force_refresh {
            if let Some(index) = self.read_fresh_cache(&cached) {
                return Ok(index);
            }
        }
        match self.fetch_index_remote() {
            Ok((index, body)) => {
                // 缓存写失败不应让命令失败：这是纯粹的加速手段。
                let _ = std::fs::write(&cached, body);
                Ok(index)
            }
            Err(remote_err) => {
                // 网络不可用时，宁可用过期缓存也不要直接失败。
                if let Ok(text) = std::fs::read_to_string(&cached) {
                    if let Ok(index) = parse_index(&text) {
                        return Ok(index);
                    }
                }
                Err(remote_err)
            }
        }
    }

    /// 从远端抓取并解析索引。
    fn fetch_index_remote(&self) -> Result<(MarketIndex, String)> {
        let resp = self
            .http
            .get(&self.source.index_url)
            .send()
            .net(format!("请求市场索引 {}", self.source.index_url))?;
        if !resp.status().is_success() {
            return Err(CliError::Network(format!(
                "市场索引返回 HTTP {}（{}）",
                resp.status(),
                self.source.index_url
            )));
        }
        let body = resp.text().net("读取市场索引内容")?;
        let index = parse_index(&body)?;
        Ok((index, body))
    }

    /// 读取缓存，且仅当未过期时返回。
    fn read_fresh_cache(&self, path: &Path) -> Option<MarketIndex> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        if modified.elapsed().ok()? > INDEX_TTL {
            return None;
        }
        let text = std::fs::read_to_string(path).ok()?;
        parse_index(&text).ok()
    }

    fn cached_index_path(&self) -> PathBuf {
        self.cache_dir.join("index.json")
    }

    /// 下载插件 zip 到缓存目录，并校验 SHA-256。
    ///
    /// 校验失败会删除临时文件并返回 [`CliError::ChecksumMismatch`]。
    pub fn download_asset(
        &self,
        repository: &Repository,
        tag: &str,
        asset: &str,
        expected_sha256: &str,
    ) -> Result<DownloadedAsset> {
        let url = repository.asset_url(tag, asset);
        let resp = self.http.get(&url).send().net(format!("下载 {tag}"))?;
        if !resp.status().is_success() {
            return Err(CliError::Network(format!(
                "下载 {tag} 失败：HTTP {}",
                resp.status()
            )));
        }

        let bytes = resp.bytes().net(format!("读取 {tag} 内容"))?;
        let actual = hex_sha256(&bytes);
        if !expected_sha256.is_empty() && !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(CliError::ChecksumMismatch {
                tag: tag.to_string(),
                expected: expected_sha256.to_string(),
                actual,
            });
        }

        let downloads = self.cache_dir.join("downloads");
        std::fs::create_dir_all(&downloads).ctx(format!("创建下载目录 {}", downloads.display()))?;
        let dest = downloads.join(sanitize_filename(asset));
        std::fs::write(&dest, &bytes).ctx(format!("写入下载文件 {}", dest.display()))?;

        Ok(DownloadedAsset {
            path: dest,
            size: bytes.len() as u64,
            sha256: actual,
        })
    }

    /// 查询某个 Release tag 下所有 asset 的下载量，返回 `asset -> count`。
    ///
    /// 静态方案下这是唯一可行的计数方式。失败时返回空表而不是报错：
    /// 下载量只是展示信息，不应该让 `plugin show --r` 整体失败。
    pub fn download_counts(
        &self,
        repository: &Repository,
        tag: &str,
    ) -> std::collections::BTreeMap<String, u64> {
        let mut out = std::collections::BTreeMap::new();
        let Some(slug) = repository.github_slug() else {
            return out;
        };
        let url = format!("https://api.github.com/repos/{slug}/releases/tags/{tag}");
        let Ok(resp) = self
            .http
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .send()
        else {
            return out;
        };
        if !resp.status().is_success() {
            return out;
        }
        let Ok(json) = resp.json::<serde_json::Value>() else {
            return out;
        };
        if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
            for asset in assets {
                let (Some(name), Some(count)) = (
                    asset.get("name").and_then(|n| n.as_str()),
                    asset.get("download_count").and_then(|c| c.as_u64()),
                ) else {
                    continue;
                };
                out.insert(name.to_string(), count);
            }
        }
        out
    }
}

/// 解析索引文本并检查 schema 版本。
pub fn parse_index(text: &str) -> Result<MarketIndex> {
    let index: MarketIndex = serde_json::from_str(text)
        .map_err(|e| CliError::Network(format!("解析市场索引失败：{e}")))?;
    if index.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(CliError::UnsupportedSchemaVersion {
            found: index.schema_version,
            supported: SUPPORTED_SCHEMA_VERSION,
        });
    }
    Ok(index)
}

/// 计算字节串的 SHA-256，返回小写十六进制。
pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 从 reader 计算 SHA-256（用于大文件流式校验的扩展点）。
#[allow(dead_code)]
pub fn hex_sha256_reader<R: Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut out = String::with_capacity(64);
    for byte in hasher.finalize() {
        out.push_str(&format!("{byte:02x}"));
    }
    Ok(out)
}

/// 去掉文件名里的路径分隔符，防止 zip entry 里的恶意路径逃逸。
pub fn sanitize_filename(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_string()
}
