//! 本地插件：扫描 `plugins/`、静态解析 `__plugin_meta__`。
//!
//! 关键约束来自 `generate_index.py`：解析 `__plugin_meta__` 时**不 import
//! 插件**。那个脚本用 Python 的 `ast` 模块做静态分析；这里在 Rust 里等价
//! 地做文本级解析——在**模块顶层**寻找 `__plugin_meta__ = PluginMetadata(...)`
//! 赋值，再取出其中的字符串字面量与字符串集合字面量。
//!
//! 之所以限定「模块顶层」，是因为 `__plugin_meta__` 按契约就是模块级声明；
//! 只看顶层能避免把插件函数体里的同名局部变量误当成元数据。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{CliError, IoContext, Result};
use crate::market::index::SemVer;

/// 插件 ID 的统一前缀（与 JianerCore `PLUGIN_NAME_PREFIX` 一致）。
pub const PLUGIN_ID_PREFIX: &str = "jianerbot-plugin-";

/// 核心内置插件：不由市场分发，级联解析时视为「已满足」。
pub const BUILTIN_PLUGIN_IDS: &[&str] = &["jianerbot-plugin-alconna"];

/// 禁用插件的前缀（与 JianerCore `DISABLED_PREFIX` 一致）。
pub const DISABLED_PREFIX: &str = "d_";

/// 从 Python 源码静态提取出的插件元数据。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PluginMeta {
    /// 插件 ID。
    pub name: String,
    /// 插件描述。
    pub description: String,
    /// 使用说明。
    pub usage: String,
    /// 依赖的插件 ID 集合。
    pub requires: Vec<String>,
}

/// 本地插件形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalKind {
    /// `plugins/Xxx.py`。
    SingleFile,
    /// `plugins/Xxx/` 目录，入口 `setup.py`。
    Package,
}

impl LocalKind {
    /// 中文标签。
    pub fn label(self) -> &'static str {
        match self {
            LocalKind::SingleFile => "单文件插件",
            LocalKind::Package => "包插件",
        }
    }
}

/// 一个本地已安装的插件。
#[derive(Debug, Clone, Serialize)]
pub struct LocalPlugin {
    /// 插件 ID；无法解析时为 None。
    pub id: Option<String>,
    /// 展示名（目录名或文件名去掉扩展名）。
    pub display_name: String,
    /// 插件形态。
    pub kind: LocalKind,
    /// 插件在磁盘上的路径。
    pub path: PathBuf,
    /// 入口文件路径。
    pub entry: PathBuf,
    /// 是否被 `d_` 前缀禁用。
    pub disabled: bool,
    /// 元数据；解析失败时为 None。
    pub meta: Option<PluginMeta>,
    /// 解析失败原因。
    pub parse_error: Option<String>,
    /// 插件目录旁边的 `market.json`（本地记录的市场版本信息）。
    pub market_version: Option<String>,
    /// 插件目录旁边的 README。
    pub readme: Option<PathBuf>,
}

impl LocalPlugin {
    /// 用于展示的 ID：解析不出时退回 `<未声明>`。
    pub fn id_label(&self) -> &str {
        self.id.as_deref().unwrap_or("<未声明>")
    }

    /// 本地记录的版本号。市场里装的插件会在 `market.json` 里带 version。
    pub fn version(&self) -> Option<SemVer> {
        self.market_version.as_deref().map(SemVer::parse)
    }
}

/// 扫描 bot 项目的 `plugins/` 目录。
///
/// 与 JianerCore 的加载器保持同样的枚举规则：
/// `__pycache__` 跳过；`d_` 前缀目录/文件视为已禁用；目录必须有 `setup.py`；
/// 单文件只认 `.py` / `.pyw`；`*.market.json` 是 Sidecar 元数据不当作插件。
pub fn scan_plugins(plugins_dir: &Path) -> Result<Vec<LocalPlugin>> {
    if !plugins_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(plugins_dir).ctx(format!("读取插件目录 {}", plugins_dir.display()))?;

    for entry in entries {
        let entry = entry.ctx("遍历插件目录")?;
        let path = entry.path();
        let raw_name = entry.file_name().to_string_lossy().to_string();

        if raw_name == "__pycache__" || raw_name.starts_with('.') {
            continue;
        }
        // market.json / Xxx.market.json 是市场 Sidecar 元数据，不是插件本体。
        if raw_name == "market.json" || raw_name.ends_with(".market.json") {
            continue;
        }

        let disabled = raw_name.starts_with(DISABLED_PREFIX);
        let effective = raw_name
            .strip_prefix(DISABLED_PREFIX)
            .unwrap_or(&raw_name)
            .to_string();

        if path.is_dir() {
            let setup = path.join("setup.py");
            if !setup.is_file() {
                continue;
            }
            out.push(build_local(
                path.clone(),
                setup,
                effective,
                disabled,
                LocalKind::Package,
            ));
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("py") | Some("pyw")
        ) {
            let display = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| effective.clone());
            out.push(build_local(
                path.clone(),
                path.clone(),
                display,
                disabled,
                LocalKind::SingleFile,
            ));
        }
    }

    out.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(out)
}

/// 组装单个本地插件的记录（含元数据解析）。
fn build_local(
    path: PathBuf,
    entry: PathBuf,
    display_name: String,
    disabled: bool,
    kind: LocalKind,
) -> LocalPlugin {
    let (meta, parse_error) = match parse_plugin_meta(&entry) {
        Ok(meta) => {
            let id = if meta.name.is_empty() {
                None
            } else {
                Some(meta.name.clone())
            };
            (Some(meta), id_validation_error(id.as_deref()))
        }
        Err(e) => (None, Some(e)),
    };

    let id = meta.as_ref().and_then(|m| {
        if m.name.is_empty() {
            None
        } else {
            Some(m.name.clone())
        }
    });

    // market.json 的位置：包插件在目录内，单文件插件在同级 Xxx.market.json。
    let market_path = match kind {
        LocalKind::Package => path.join("market.json"),
        LocalKind::SingleFile => {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
            match stem {
                Some(stem) => path.with_file_name(format!("{stem}.market.json")),
                None => path.with_extension("market.json"),
            }
        }
    };
    let market_version = std::fs::read_to_string(&market_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get("version")
                .and_then(|s| s.as_str())
                .map(str::to_string)
        });

    let readme = {
        let candidate = match kind {
            LocalKind::Package => path.join("README.md"),
            LocalKind::SingleFile => path.with_file_name("README.md"),
        };
        candidate.is_file().then_some(candidate)
    };

    LocalPlugin {
        id,
        display_name,
        kind,
        path,
        entry,
        disabled,
        meta,
        parse_error,
        market_version,
        readme,
    }
}

/// 校验 ID 是否符合 `jianerbot-plugin-*` 约定，返回错误消息。
fn id_validation_error(id: Option<&str>) -> Option<String> {
    match id {
        None => Some("缺少 __plugin_meta__ 的 name".to_string()),
        Some(name) if !is_valid_plugin_id(name) => Some(format!(
            "插件 ID '{name}' 不符合约定（应为 {PLUGIN_ID_PREFIX}<小写字母/数字/连字符>）"
        )),
        Some(_) => None,
    }
}

/// 判断插件 ID 是否合法。
pub fn is_valid_plugin_id(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(PLUGIN_ID_PREFIX) else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let mut parts = rest.split('-');
    parts.all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

/// 静态解析入口文件里的 `__plugin_meta__`。
///
/// 找不到声明时返回错误（与 `generate_index.py` 的行为一致）。
pub fn parse_plugin_meta(entry: &Path) -> std::result::Result<PluginMeta, String> {
    let source = std::fs::read_to_string(entry)
        .map_err(|e| format!("读取 {} 失败：{e}", entry.display()))?;
    parse_plugin_meta_source(&source).ok_or_else(|| {
        format!(
            "{}: 缺少 __plugin_meta__ = PluginMetadata(...) 声明",
            entry.display()
        )
    })
}

/// 从源码文本解析 `__plugin_meta__`。
pub fn parse_plugin_meta_source(source: &str) -> Option<PluginMeta> {
    let call = find_plugin_meta_call(source)?;
    let mut meta = PluginMeta::default();
    for (key, value) in parse_call_kwargs(&call) {
        match key.as_str() {
            "name" => meta.name = value.as_str().unwrap_or_default(),
            "description" => meta.description = value.as_str().unwrap_or_default(),
            "usage" => meta.usage = value.as_str().unwrap_or_default(),
            "requires" => {
                let mut set: BTreeSet<String> = value
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                meta.requires = std::mem::take(&mut set).into_iter().collect();
            }
            _ => {}
        }
    }
    // name 缺失时仍然返回，由调用方判定为无效元数据。
    Some(meta)
}

/// 在一行里定位 `__plugin_meta__ = PluginMetadata(...)` 的括号内容。
///
/// 只扫描模块顶层（列 0）的赋值语句，避免命中函数体内的同名变量。
fn find_plugin_meta_call(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        // 顶层语句不应有前导空白；缩进说明它在函数/类体内。
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("__plugin_meta__") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with("PluginMetadata") {
            continue;
        }

        // 收集从当前行起的文本，直到括号配平。
        let mut buf = String::new();
        for l in lines.iter().skip(idx) {
            buf.push_str(l);
            buf.push('\n');
            if paren_balanced(&buf) {
                return Some(buf);
            }
            // 防御：声明不可能长到 400 行。
            if buf.len() > 200_000 {
                break;
            }
        }
        return None;
    }
    None
}

/// 检查文本里的圆括号是否配平（忽略字符串字面量内的括号）。
fn paren_balanced(text: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_str: Option<char> = None;
    let mut prev_backslash = false;
    for c in text.chars() {
        if let Some(quote) = in_str {
            if prev_backslash {
                prev_backslash = false;
                continue;
            }
            if c == '\\' {
                prev_backslash = true;
            } else if c == quote {
                in_str = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => in_str = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth <= 0 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 从 `PluginMetadata(...)` 调用文本里取出 `key=value` 关键字参数。
fn parse_call_kwargs(call: &str) -> Vec<(String, LiteralValue)> {
    let Some(open) = call.find('(') else {
        return Vec::new();
    };
    let inner = &call[open + 1..];
    let mut out = Vec::new();
    for chunk in split_top_level(inner) {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((key, value)) = chunk.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        if let Some(lit) = parse_literal(value.trim()) {
            out.push((key.to_string(), lit));
        }
    }
    out
}

/// 按顶层逗号切分参数（跳过字符串与嵌套括号内部）。
fn split_top_level(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    let mut in_str: Option<char> = None;
    let mut prev_backslash = false;

    for c in inner.chars() {
        if let Some(quote) = in_str {
            current.push(c);
            if prev_backslash {
                prev_backslash = false;
            } else if c == '\\' {
                prev_backslash = true;
            } else if c == quote {
                in_str = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                in_str = Some(c);
                current.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' => {
                // 最外层 `PluginMetadata(` 的右括号，到此结束。
                if depth == 0 {
                    parts.push(current);
                    return parts;
                }
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

/// 支持的字面量：字符串、字符串集合、字符串列表、数字、布尔。
#[derive(Debug, Clone)]
enum LiteralValue {
    Str(String),
    List(Vec<LiteralValue>),
}

impl LiteralValue {
    fn as_str(&self) -> Option<String> {
        match self {
            LiteralValue::Str(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[LiteralValue]> {
        match self {
            LiteralValue::List(items) => Some(items),
            _ => None,
        }
    }
}

/// 解析一个字面量表达式的文本形式。
///
/// 能处理 `"abc"`、`{'a', 'b'}`、`["a"]` 三种插件元数据里实际出现的写法。
/// 相邻字符串字面量的隐式拼接（Python 的 `"a" "b"`）也会被合并，
/// 这在 `usage=(...)` 里很常见。
fn parse_literal(text: &str) -> Option<LiteralValue> {
    let text = text.trim().trim_matches(|c: char| c.is_whitespace());
    if text.is_empty() {
        return None;
    }
    // 去掉包裹的括号（`usage=( ... )` 或 `requires={ ... }` 的续行写法）。
    let text = strip_outer_wrappers(text);
    let text = text.trim();

    if let Some(chunks) = collect_adjacent_strings(text) {
        return Some(LiteralValue::Str(chunks));
    }

    let open = text.find(['[', '{'])?;
    let close = text.rfind([']', '}'])?;
    if close <= open {
        return None;
    }
    let inner = &text[open + 1..close];
    let items: Vec<LiteralValue> = split_top_level(inner)
        .into_iter()
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                None
            } else {
                parse_literal(part)
            }
        })
        .collect();
    Some(LiteralValue::List(items))
}

/// 去掉一层或多层包裹括号，但保留集合/列表字面量本身。
fn strip_outer_wrappers(text: &str) -> &str {
    let mut current = text.trim();
    loop {
        if !(current.starts_with('(') && current.ends_with(')')) {
            return current;
        }
        let inner = &current[1..current.len() - 1];
        // 只有当中括号在内部也配平时才剥掉，避免破坏 `{...}` 的语义。
        if paren_balanced(inner) {
            current = inner.trim();
        } else {
            return current;
        }
    }
}

/// 把连续的字符串字面量拼成一个字符串（处理 Python 的隐式拼接）。
fn collect_adjacent_strings(text: &str) -> Option<String> {
    let mut chars = text.chars().peekable();
    let mut out = String::new();
    let mut found_any = false;

    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        let Some(&quote) = chars.peek() else { break };
        if quote != '\'' && quote != '"' {
            break;
        }
        chars.next();

        // 三引号字符串。
        let triple = {
            let mut clone = chars.clone();
            clone.peek() == Some(&quote) && {
                let mut c2 = clone.clone();
                c2.next();
                c2.peek() == Some(&quote)
            }
        };
        if triple {
            chars.next();
            chars.next();
        }

        let mut terminated = false;
        let mut prev_backslash = false;
        while let Some(c) = chars.next() {
            if prev_backslash {
                out.push(unescape(c));
                prev_backslash = false;
                continue;
            }
            if c == '\\' {
                prev_backslash = true;
                continue;
            }
            if c == quote {
                if triple {
                    let mut clone = chars.clone();
                    if clone.peek() == Some(&quote) {
                        clone.next();
                        if clone.peek() == Some(&quote) {
                            chars.next();
                            chars.next();
                            terminated = true;
                            break;
                        }
                    }
                    out.push(c);
                } else {
                    terminated = true;
                    break;
                }
            } else {
                out.push(c);
            }
        }
        if !terminated {
            return None;
        }
        found_any = true;
    }

    if found_any {
        // 确认剩下的内容是空白，否则不是纯字符串字面量。
        let rest: String = chars.collect();
        if rest.trim().is_empty() {
            return Some(out);
        }
    }
    None
}

/// 处理常见转义序列。
fn unescape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        other => other,
    }
}

/// 在本地插件列表里按 ID 或显示名模糊匹配。
///
/// 返回候选列表，精确命中时只返回那一个。
pub fn match_local<'a>(plugins: &'a [LocalPlugin], query: &str) -> Vec<&'a LocalPlugin> {
    let q = query.to_lowercase();
    let mut exact = Vec::new();
    let mut fuzzy = Vec::new();

    for plugin in plugins {
        let id = plugin.id.clone().unwrap_or_default().to_lowercase();
        let display = plugin.display_name.to_lowercase();
        let short = id.strip_prefix(PLUGIN_ID_PREFIX).unwrap_or(&id);

        if id == q || short == q || display == q {
            exact.push(plugin);
        } else if id.contains(&q) || short.contains(&q) || display.contains(&q) {
            fuzzy.push(plugin);
        }
    }

    if !exact.is_empty() {
        exact
    } else {
        fuzzy
    }
}

/// 解析级联依赖：返回安装顺序（依赖在前），并报告缺失项。
///
/// `installed` 是本地已有的插件 ID 集合，`builtins` 是内置插件 ID 集合；
/// 两者都视为已满足，不会被重复拉取。
pub fn resolve_dependencies(
    root: &str,
    lookup: &dyn Fn(&str) -> Option<Vec<String>>,
    installed: &BTreeSet<String>,
    builtins: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let mut order = Vec::new();
    let mut visited = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut stack = Vec::new();

    visit(
        root,
        lookup,
        installed,
        builtins,
        &mut visited,
        &mut order,
        &mut missing,
        &mut stack,
    )?;

    if !missing.is_empty() {
        return Err(CliError::PluginNotFound(format!(
            "依赖的插件在市场中不存在：{}",
            missing.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(order)
}

#[allow(clippy::too_many_arguments)]
fn visit(
    id: &str,
    lookup: &dyn Fn(&str) -> Option<Vec<String>>,
    installed: &BTreeSet<String>,
    builtins: &BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    order: &mut Vec<String>,
    missing: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> Result<()> {
    if installed.contains(id) || builtins.contains(id) || !visited.insert(id.to_string()) {
        return Ok(());
    }
    if stack.iter().any(|s| s == id) {
        // 依赖成环：不再深入，交由上层报错。
        return Err(CliError::PluginNotFound(format!(
            "插件依赖存在环：{} -> {id}",
            stack.join(" -> ")
        )));
    }
    stack.push(id.to_string());
    if let Some(deps) = lookup(id) {
        for dep in deps {
            visit(
                &dep, lookup, installed, builtins, visited, order, missing, stack,
            )?;
        }
        order.push(id.to_string());
    } else {
        missing.insert(id.to_string());
    }
    stack.pop();
    Ok(())
}

/// 读取内置插件 ID 集合。
pub fn builtin_ids() -> BTreeSet<String> {
    BUILTIN_PLUGIN_IDS.iter().map(|s| s.to_string()).collect()
}
