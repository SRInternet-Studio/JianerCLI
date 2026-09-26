//! 统一错误类型。
//!
//! JianerCLI 的所有子命令都返回 [`Result`]，由 `main` 统一渲染成人话。
//! 这里刻意区分「用户可自行修复」的错误（配置缺失、插件不存在、网络不通）
//! 与「程序 bug」（IO/序列化失败），因为前者应该只打印一行建议而不是 backtrace。

use std::path::PathBuf;

/// JianerCLI 的错误枚举。
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// 当前目录不是可识别的 Jianer 项目，或缺少关键文件。
    #[error("未在 {path} 找到{kind}")]
    NotAProject {
        /// 被检查的目录。
        path: PathBuf,
        /// 期望的项目种类描述，例如 "Jianer_QQ_bot 实例"。
        kind: String,
    },

    /// 需要 Jianer_QQ_bot 实例才能运行的命令被用在了 Core 项目上。
    #[error("该命令需要 Jianer_QQ_bot 实例，但当前目录是 JianerCore 项目")]
    BotFeatureUnavailable,

    /// 市场索引里没有这个插件。
    #[error("智慧市场中找不到插件 '{0}'")]
    PluginNotFound(String),

    /// 模糊匹配命中多个插件，需要用户明确指定。
    #[error("'{query}' 匹配到多个插件：{candidates}")]
    AmbiguousPlugin {
        /// 用户输入的查询串。
        query: String,
        /// 候选插件 ID，逗号分隔。
        candidates: String,
    },

    /// 本地没有安装该插件。
    #[error("本地未安装插件 '{0}'")]
    PluginNotInstalled(String),

    /// 下载到的插件包校验失败，说明传输损坏或索引被篡改。
    #[error("{tag} 的插件包校验失败：期望 sha256 {expected}，实际 {actual}")]
    ChecksumMismatch {
        /// Release tag。
        tag: String,
        /// 索引里声明的摘要。
        expected: String,
        /// 实际下载内容的摘要。
        actual: String,
    },

    /// 市场索引格式不兼容。
    #[error(
        "智慧市场索引 schemaVersion={found} 不受支持（本版本支持 {supported}），请升级 JianerCLI"
    )]
    UnsupportedSchemaVersion {
        /// 索引里声明的版本。
        found: u64,
        /// 本程序支持的版本。
        supported: u64,
    },

    /// 网络请求失败。
    #[error("网络请求失败：{0}")]
    Network(String),

    /// 项目里缺少更新配置，需要先跑 `update-conf`。
    #[error("项目尚未配置更新渠道，请先运行 `jianer-cli update-conf`")]
    UpdateNotConfigured,

    /// 交互式向导被用户取消（Ctrl-C / Esc）。
    #[error("操作已取消")]
    Cancelled,

    /// Python 环境不可用（缺 venv、缺 pip 等）。
    #[error("{0}")]
    PythonEnv(String),

    /// 兜底 IO 错误。
    #[error("{context}：{source}")]
    Io {
        /// 出错时正在做什么。
        context: String,
        /// 底层错误。
        #[source]
        source: std::io::Error,
    },
}

impl CliError {
    /// 为 IO 错误附加上下文，便于定位是哪个文件/目录出的问题。
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        CliError::Io {
            context: context.into(),
            source,
        }
    }
}

/// 给 `Result` 加 IO 上下文的便捷 trait。
pub trait IoContext<T> {
    /// 出错时记录「正在做什么」。
    fn ctx(self, context: impl Into<String>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn ctx(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| CliError::io(context, e))
    }
}

/// 给 `Result` 加网络请求上下文的便捷 trait。
pub trait NetContext<T> {
    /// 把 reqwest 错误折叠成 [`CliError::Network`]。
    fn net(self, context: impl AsRef<str>) -> Result<T>;
}

impl<T> NetContext<T> for std::result::Result<T, reqwest::Error> {
    fn net(self, context: impl AsRef<str>) -> Result<T> {
        self.map_err(|e| CliError::Network(format!("{}：{e}", context.as_ref())))
    }
}

/// JianerCLI 的统一结果类型。
pub type Result<T> = std::result::Result<T, CliError>;

/// 把 JSON 错误包成可读消息。
pub fn json_err(context: impl AsRef<str>, e: serde_json::Error) -> CliError {
    CliError::Network(format!("{}：JSON 解析失败：{e}", context.as_ref()))
}
