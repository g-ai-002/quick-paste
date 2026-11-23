//! 内容源：本地 YAML 或 Gitea / Gitee / GitHub / GitLab 上的远端 YAML。
//!
//! 远端文件只提供 `text` / `tags`；`usage_count` / `is_pinned` 仍保存在
//! `%LOCALAPPDATA%\QuickPaste\memos.yaml`，因此同一份远端备忘录可以在多台设备间共享，
//! 但使用次数和置顶状态按设备各自维护。

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{logging, memo};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Local,
    Gitea,
    Gitee,
    GitHub,
    GitLab,
}

impl SourceKind {
    pub const ALL: [Self; 5] = [
        Self::Local,
        Self::Gitea,
        Self::Gitee,
        Self::GitHub,
        Self::GitLab,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "本地文件",
            Self::Gitea => "Gitea",
            Self::Gitee => "Gitee",
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
        }
    }

    pub fn is_remote(self) -> bool {
        !matches!(self, Self::Local)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DataSourceConfig {
    pub kind: SourceKind,
    /// 本地 YAML 文件路径；留空时自动查找当前工作目录和程序目录。
    pub local_path: String,
    /// 远端 YAML 文件页面或 raw 地址。
    pub url: String,
    /// 私有仓库的访问令牌；公开仓库可以留空。
    pub token: String,
}

impl DataSourceConfig {
    pub fn normalized(mut self) -> Self {
        self.local_path = self.local_path.trim().to_owned();
        self.url = self.url.trim().to_owned();
        self.token = self.token.trim().to_owned();
        self
    }

    /// 校验远端配置；本地数据源始终有效。
    pub fn validation_error(&self) -> Option<String> {
        if self.kind == SourceKind::Local {
            return None;
        }

        let url = self.url.trim();
        if url.is_empty() {
            return Some("请填写 YAML 文件 URL".to_owned());
        }

        let Some(parts) = UrlParts::parse(url) else {
            return Some("URL 格式无效，需要以 http:// 或 https:// 开头".to_owned());
        };
        let https = parts.scheme.eq_ignore_ascii_case("https");
        let http = parts.scheme.eq_ignore_ascii_case("http");

        match self.kind {
            SourceKind::Local => None,
            SourceKind::GitHub => {
                if !https {
                    return Some("GitHub 数据源必须使用 https 地址".to_owned());
                }
                if !is_github_host(parts.host) {
                    return Some(
                        "GitHub 数据源必须使用 github.com 或 raw.githubusercontent.com 的地址"
                            .to_owned(),
                    );
                }
                if parts.host.eq_ignore_ascii_case("api.github.com")
                    && !parts.path.starts_with("/repos/")
                {
                    return Some("GitHub API 地址必须是 /repos/ 下的文件地址".to_owned());
                }
                if parts.host.eq_ignore_ascii_case("github.com")
                    && !is_github_file_path(parts.path)
                {
                    return Some(
                        "请填写 GitHub 文件页面（/blob/）或 raw 文件地址".to_owned(),
                    );
                }
                None
            }
            SourceKind::Gitee => {
                if !https {
                    return Some("Gitee 数据源必须使用 https 地址".to_owned());
                }
                if !is_gitee_host(parts.host) {
                    return Some("Gitee 数据源必须使用 gitee.com 的地址".to_owned());
                }
                if !is_gitee_file_path(parts.path) {
                    return Some("请填写 Gitee 文件页面（/blob/）或 raw 文件地址".to_owned());
                }
                None
            }
            SourceKind::GitLab => {
                if !http && !https {
                    return Some("GitLab 数据源必须使用 http:// 或 https:// 地址".to_owned());
                }
                if is_gitee_host(parts.host) || is_github_host(parts.host) {
                    return Some(
                        "GitLab 地址不能指向 Gitee 或 GitHub，请切换数据源类型".to_owned(),
                    );
                }
                if parts.host.eq_ignore_ascii_case("gitlab.com") && !https {
                    return Some("gitlab.com 必须使用 https 地址".to_owned());
                }
                if !is_gitlab_file_path(parts.path) {
                    return Some(
                        "请填写 GitLab 文件页面（/-/blob/）、raw 地址或 /api/v4/.../raw 地址"
                            .to_owned(),
                    );
                }
                None
            }
            SourceKind::Gitea => {
                if !http && !https {
                    return Some("Gitea 数据源必须使用 http:// 或 https:// 地址".to_owned());
                }
                if is_gitee_host(parts.host) || is_github_host(parts.host) {
                    return Some(
                        "Gitea 地址不能指向 Gitee 或 GitHub，请切换数据源类型".to_owned(),
                    );
                }
                if !is_gitea_file_path(parts.path) {
                    return Some("请填写 Gitea 文件页面（/src/）或 raw 文件地址".to_owned());
                }
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentSource {
    Local(PathBuf),
    Remote { kind: SourceKind, url: String },
}

impl ContentSource {
    pub fn resolve(config: &DataSourceConfig, data_path: &Path) -> Self {
        match config.kind {
            SourceKind::Local => {
                let configured = config.local_path.trim();
                let path = if configured.is_empty() {
                    memo::external_source_path()
                        .filter(|path| path.as_path() != data_path)
                        .unwrap_or_else(|| data_path.to_path_buf())
                } else {
                    PathBuf::from(configured)
                };
                Self::Local(path)
            }
            kind => Self::Remote {
                kind,
                url: config.url.trim().to_owned(),
            },
        }
    }

    pub fn kind(&self) -> SourceKind {
        match self {
            Self::Local(_) => SourceKind::Local,
            Self::Remote { kind, .. } => *kind,
        }
    }

    pub fn is_remote(&self) -> bool {
        self.kind().is_remote()
    }

    pub fn local_path(&self) -> Option<&Path> {
        match self {
            Self::Local(path) => Some(path),
            Self::Remote { .. } => None,
        }
    }

    /// 展示用地址：本地路径，或去掉 `access_token` 的远端 URL。
    pub fn location(&self) -> String {
        match self {
            Self::Local(path) => path.display().to_string(),
            Self::Remote { kind, url } => {
                format!("{}：{}", kind.label(), redact_url(url))
            }
        }
    }

    /// 可以在浏览器打开的公开地址；本地数据源返回 `None`。
    pub fn public_url(&self) -> Option<String> {
        match self {
            Self::Local(_) => None,
            Self::Remote { url, .. } => Some(redact_url(url)),
        }
    }
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("数据源配置无效：{0}")]
    Config(String),
    #[error("读取本地数据文件失败：{0}")]
    Local(memo::MemoError),
    #[error("请求远端数据失败：{0}")]
    Request(String),
    #[error("远端 YAML 内容无效：{0}")]
    Content(memo::MemoError),
}

/// 读取内容源并与本地状态合并；远端读取失败时返回错误，由调用方决定是否使用缓存。
pub fn load_merged(
    source: &ContentSource,
    config: &DataSourceConfig,
    data_path: &Path,
) -> Result<Vec<memo::Memo>, SourceError> {
    logging::write(format!("加载内容源：{}", source.location()));
    logging::write(format!("本地状态：{}", data_path.display()));

    let mut memos = match source {
        ContentSource::Local(source_path) => {
            if !config.local_path.trim().is_empty() && !source_path.exists() {
                return Err(SourceError::Config(format!(
                    "找不到本地数据文件：{}",
                    source_path.display()
                )));
            }

            if source_path.as_path() != data_path && source_path.exists() {
                let source_memos = memo::load(source_path).map_err(SourceError::Local)?;
                logging::write(format!(
                    "本地内容源读取成功：{} 条",
                    source_memos.len()
                ));
                let local = memo::load_or_create(data_path).map_err(SourceError::Local)?;
                logging::write(format!("本地状态读取成功：{} 条", local.len()));
                memo::merge(&source_memos, &local)
            } else {
                memo::load_or_create(data_path).map_err(SourceError::Local)?
            }
        }
        ContentSource::Remote { kind, url } => {
            if let Some(error) = config.validation_error() {
                return Err(SourceError::Config(error));
            }
            let source_memos = fetch_remote(*kind, url, &config.token)?;
            let local = memo::load_or_create(data_path).map_err(SourceError::Local)?;
            logging::write(format!("本地状态读取成功：{} 条", local.len()));
            memo::merge(&source_memos, &local)
        }
    };

    memo::sort_for_display(&mut memos);
    logging::write(format!("备忘录加载完成：{} 条", memos.len()));
    Ok(memos)
}

fn fetch_remote(kind: SourceKind, url: &str, token: &str) -> Result<Vec<memo::Memo>, SourceError> {
    let endpoint = request_url(kind, url, token);
    logging::write(format!(
        "读取远端内容源：{} {}",
        kind.label(),
        redact_url(url)
    ));

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .user_agent(concat!("quick-paste/", env!("CARGO_PKG_VERSION")))
        .build();
    let mut request = agent.get(&endpoint);

    if !token.trim().is_empty() {
        match kind {
            SourceKind::GitHub => {
                request = request.set("Authorization", &format!("Bearer {token}"));
            }
            SourceKind::Gitea => {
                request = request.set("Authorization", &format!("token {token}"));
            }
            SourceKind::GitLab => {
                request = request.set("PRIVATE-TOKEN", token);
            }
            SourceKind::Gitee | SourceKind::Local => {}
        }
    }
    if kind == SourceKind::GitHub {
        request = request.set("Accept", "application/vnd.github.raw");
    }

    let response = request.call().map_err(|error| {
        let message = match error {
            ureq::Error::Status(401 | 403, _) => {
                "HTTP 401/403：TOKEN 无权访问或已过期".to_owned()
            }
            ureq::Error::Status(404, _) => {
                "HTTP 404：文件不存在或 URL 不正确".to_owned()
            }
            ureq::Error::Status(status, _) => format!("HTTP {status}"),
            other => format!("网络请求失败：{}", redact_url(&other.to_string())),
        };
        SourceError::Request(message)
    })?;

    let content = response
        .into_string()
        .map_err(|error| SourceError::Request(format!("读取响应失败：{error}")))?;
    let memos = memo::parse(&content).map_err(SourceError::Content)?;
    logging::write(format!(
        "远端内容源读取成功：{} 条（{}）",
        memos.len(),
        kind.label()
    ));
    Ok(memos)
}

fn request_url(kind: SourceKind, url: &str, token: &str) -> String {
    let url = normalized_remote_url(kind, url);
    if kind == SourceKind::Gitee && !token.trim().is_empty() && !url.contains("access_token=") {
        let separator = if url.contains('?') { '&' } else { '?' };
        format!("{url}{separator}access_token={}", percent_encode(token))
    } else {
        url
    }
}

/// 把常见的文件页面地址转换为真正的 raw 文件地址；无法识别时保留原地址。
fn normalized_remote_url(kind: SourceKind, url: &str) -> String {
    let normalized = match kind {
        SourceKind::GitHub => normalize_github_url(url),
        SourceKind::Gitee => normalize_gitee_url(url),
        SourceKind::Gitea => normalize_gitea_url(url),
        SourceKind::GitLab => normalize_gitlab_url(url),
        SourceKind::Local => None,
    };
    normalized.unwrap_or_else(|| url.trim().to_owned())
}

fn normalize_github_url(url: &str) -> Option<String> {
    let parts = UrlParts::parse(url)?;
    if !(parts.host.eq_ignore_ascii_case("github.com")
        || parts.host.eq_ignore_ascii_case("www.github.com"))
    {
        return None;
    }
    let segments: Vec<&str> = parts.path.trim_start_matches('/').split('/').collect();
    let [owner, repo, mode, rest @ ..] = segments.as_slice() else {
        return None;
    };
    if !(mode.eq_ignore_ascii_case("blob") || mode.eq_ignore_ascii_case("raw")) {
        return None;
    }
    let (branch, path) = rest.split_first()?;
    if owner.is_empty() || repo.is_empty() || branch.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{}",
        path.join("/")
    ))
}

fn normalize_gitee_url(url: &str) -> Option<String> {
    let parts = UrlParts::parse(url)?;
    if !is_gitee_host(parts.host) {
        return None;
    }
    let segments: Vec<&str> = parts.path.trim_start_matches('/').split('/').collect();
    let [owner, repo, mode, rest @ ..] = segments.as_slice() else {
        return None;
    };
    if !(mode.eq_ignore_ascii_case("blob") || mode.eq_ignore_ascii_case("raw")) {
        return None;
    }
    let (branch, path) = rest.split_first()?;
    if owner.is_empty() || repo.is_empty() || branch.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!(
        "https://gitee.com/api/v5/repos/{owner}/{repo}/raw/{}?ref={}",
        path.join("/"),
        percent_encode(branch)
    ))
}

fn normalize_gitea_url(url: &str) -> Option<String> {
    let parts = UrlParts::parse(url)?;
    if is_gitee_host(parts.host) || is_github_host(parts.host) {
        return None;
    }
    let segments: Vec<&str> = parts.path.trim_start_matches('/').split('/').collect();
    let [owner, repo, mode, rest @ ..] = segments.as_slice() else {
        return None;
    };
    if !mode.eq_ignore_ascii_case("src") {
        return None;
    }
    let (reference_kind, rest) = rest.split_first()?;
    if !(reference_kind.eq_ignore_ascii_case("branch")
        || reference_kind.eq_ignore_ascii_case("tag")
        || reference_kind.eq_ignore_ascii_case("commit"))
    {
        return None;
    }
    let (reference, path) = rest.split_first()?;
    if owner.is_empty() || repo.is_empty() || reference.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!(
        "{}://{}/{owner}/{repo}/raw/{reference_kind}/{reference}/{}",
        parts.scheme,
        parts.authority,
        path.join("/")
    ))
}


fn normalize_gitlab_url(url: &str) -> Option<String> {
    let parts = UrlParts::parse(url)?;
    if parts.path.contains("/api/v4/projects/") {
        return None;
    }
    let (project_prefix, suffix) = parts.path.split_once("/-/")?;
    let mut suffix = suffix.split('/');
    let mode = suffix.next()?;
    if !(mode.eq_ignore_ascii_case("blob") || mode.eq_ignore_ascii_case("raw")) {
        return None;
    }
    let reference = suffix.next()?;
    let file_path = suffix.collect::<Vec<_>>().join("/");
    let project_path = project_prefix.trim_start_matches('/');
    if project_path.is_empty() || reference.is_empty() || file_path.is_empty() {
        return None;
    }
    Some(format!(
        "{}://{}/api/v4/projects/{}/repository/files/{}/raw?ref={}",
        parts.scheme,
        parts.authority,
        percent_encode(project_path),
        percent_encode(&file_path),
        percent_encode(reference),
    ))
}

fn is_github_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("github.com")
        || host.eq_ignore_ascii_case("www.github.com")
        || host.eq_ignore_ascii_case("api.github.com")
        || host.eq_ignore_ascii_case("raw.githubusercontent.com")
}

fn is_gitee_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("gitee.com") || host.eq_ignore_ascii_case("www.gitee.com")
}

fn is_github_file_path(path: &str) -> bool {
    path.contains("/blob/") || path.contains("/raw/")
}

fn is_gitee_file_path(path: &str) -> bool {
    path.contains("/blob/") || path.contains("/raw/")
}

fn is_gitea_file_path(path: &str) -> bool {
    path.contains("/raw/") || path.contains("/src/")
}

fn is_gitlab_file_path(path: &str) -> bool {
    path.contains("/-/raw/")
        || path.contains("/-/blob/")
        || (path.contains("/api/v4/projects/")
            && path.contains("/repository/files/")
            && (path.ends_with("/raw") || path.ends_with("/raw/")))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// 避免把访问令牌写进日志、错误提示或关于页。
fn redact_url(url: &str) -> String {
    let url = redact_query_key(url, "access_token=");
    redact_query_key(&url, "private_token=")
}

fn redact_query_key(url: &str, key: &str) -> String {
    let mut result = String::with_capacity(url.len());
    let mut rest = url;
    while let Some(index) = rest.find(key) {
        result.push_str(&rest[..index]);
        result.push_str(key);
        result.push_str("***");
        let after = &rest[index + key.len()..];
        if let Some(end) = after.find('&') {
            rest = &after[end..];
        } else {
            rest = "";
        }
    }
    result.push_str(rest);
    result
}

struct UrlParts<'a> {
    scheme: &'a str,
    authority: &'a str,
    host: &'a str,
    path: &'a str,
}

impl<'a> UrlParts<'a> {
    fn parse(value: &'a str) -> Option<Self> {
        let value = value.trim();
        let (scheme, rest) = value.split_once("://")?;
        if scheme.is_empty() || rest.is_empty() {
            return None;
        }

        let rest = rest.split('#').next().unwrap_or(rest);
        let authority_end = rest
            .find(|ch| ch == '/' || ch == '?')
            .unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if authority.is_empty() {
            return None;
        }

        let host_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = if let Some(stripped) = host_port.strip_prefix('[') {
            let end = stripped.find(']')?;
            &host_port[..end + 2]
        } else {
            host_port.split(':').next().unwrap_or(host_port)
        };
        if host.is_empty() {
            return None;
        }

        let after = &rest[authority_end..];
        let path = after.split('?').next().unwrap_or(after);
        let path = if path.is_empty() { "/" } else { path };
        Some(Self {
            scheme,
            authority,
            host,
            path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(kind: SourceKind, url: &str) -> DataSourceConfig {
        DataSourceConfig {
            kind,
            url: url.to_owned(),
            ..DataSourceConfig::default()
        }
    }

    #[test]
    fn accepts_matching_remote_hosts() {
        assert!(
            config(
                SourceKind::Gitee,
                "https://gitee.com/owner/repo/blob/main/memos.yaml"
            )
            .validation_error()
            .is_none()
        );
        assert!(
            config(
                SourceKind::GitHub,
                "https://github.com/owner/repo/blob/main/memos.yaml"
            )
            .validation_error()
            .is_none()
        );
        assert!(
            config(
                SourceKind::GitHub,
                "https://raw.githubusercontent.com/owner/repo/main/memos.yaml"
            )
            .validation_error()
            .is_none()
        );
        assert!(
            config(
                SourceKind::Gitea,
                "http://192.168.31.120:3000/owner/repo/raw/branch/main/memos.yaml"
            )
            .validation_error()
            .is_none()
        );
    }

    #[test]
    fn rejects_wrong_remote_hosts() {
        assert!(
            config(
                SourceKind::Gitee,
                "https://github.com/owner/repo/blob/main/memos.yaml"
            )
            .validation_error()
            .is_some()
        );
        assert!(
            config(
                SourceKind::GitHub,
                "https://gitee.com/owner/repo/blob/main/memos.yaml"
            )
            .validation_error()
            .is_some()
        );
        assert!(
            config(
                SourceKind::Gitea,
                "https://gitee.com/owner/repo/blob/main/memos.yaml"
            )
            .validation_error()
            .is_some()
        );
        assert!(config(SourceKind::GitHub, "ftp://github.com/x").validation_error().is_some());
    }

    #[test]
    fn rejects_host_only_and_unknown_paths() {
        assert!(
            config(
                SourceKind::Gitee,
                "https://gitee.com/owner/repo"
            )
            .validation_error()
            .is_some()
        );
        assert!(
            config(
                SourceKind::GitHub,
                "https://github.com/owner/repo"
            )
            .validation_error()
            .is_some()
        );
        assert!(
            config(
                SourceKind::Gitea,
                "https://gitea.example.com/owner/repo"
            )
            .validation_error()
            .is_some()
        );
    }

    #[test]
    fn normalizes_file_pages_to_raw_urls() {
        assert_eq!(
            normalized_remote_url(
                SourceKind::GitHub,
                "https://github.com/owner/repo/blob/main/memos.yaml"
            ),
            "https://raw.githubusercontent.com/owner/repo/main/memos.yaml"
        );
        assert_eq!(
            normalized_remote_url(
                SourceKind::Gitee,
                "https://gitee.com/owner/repo/blob/main/memos.yaml"
            ),
            "https://gitee.com/api/v5/repos/owner/repo/raw/memos.yaml?ref=main"
        );
        assert_eq!(
            normalized_remote_url(
                SourceKind::Gitea,
                "http://192.168.31.120:3000/owner/repo/src/branch/main/memos.yaml"
            ),
            "http://192.168.31.120:3000/owner/repo/raw/branch/main/memos.yaml"
        );
    }

    #[test]
    fn keeps_raw_and_api_urls_unchanged() {
        let github = "https://raw.githubusercontent.com/owner/repo/main/memos.yaml";
        assert_eq!(
            normalized_remote_url(SourceKind::GitHub, github),
            github
        );
        let gitea = "https://gitea.example.com/owner/repo/raw/branch/main/memos.yaml";
        assert_eq!(normalized_remote_url(SourceKind::Gitea, gitea), gitea);
        let gitee_api =
            "https://gitee.com/api/v5/repos/owner/repo/raw/memos.yaml?ref=main";
        assert_eq!(
            normalized_remote_url(SourceKind::Gitee, gitee_api),
            gitee_api
        );
    }

    #[test]
    fn appends_gitee_token_and_encodes_it() {
        let url = request_url(
            SourceKind::Gitee,
            "https://gitee.com/owner/repo/blob/main/memos.yaml",
            "a+b/c=",
        );
        assert_eq!(
            url,
            "https://gitee.com/api/v5/repos/owner/repo/raw/memos.yaml?ref=main&access_token=a%2Bb%2Fc%3D"
        );
    }

    #[test]
    fn redacts_tokens_in_display_urls() {
        assert_eq!(
            redact_url("https://gitee.com/x?access_token=secret&ref=main"),
            "https://gitee.com/x?access_token=***&ref=main"
        );
        assert_eq!(
            redact_url("https://gitlab.com/api/v4/projects/x/raw/private_token=secret"),
            "https://gitlab.com/api/v4/projects/x/raw/private_token=***"
        );
    }


    #[test]
    fn accepts_self_hosted_gitlab_urls() {
        assert!(
            config(
                SourceKind::GitLab,
                "http://gitlab.example.com/group/repo/-/blob/main/memos.yaml"
            )
            .validation_error()
            .is_none()
        );
        assert!(
            config(
                SourceKind::GitLab,
                "https://gitlab.com/group/repo/-/raw/main/memos.yaml"
            )
            .validation_error()
            .is_none()
        );
        assert!(
            config(
                SourceKind::GitLab,
                "https://gitlab.example.com/api/v4/projects/group%2Frepo/repository/files/memos.yaml/raw?ref=main"
            )
            .validation_error()
            .is_none()
        );
    }

    #[test]
    fn normalizes_gitlab_web_urls_to_api_raw_urls() {
        assert_eq!(
            normalized_remote_url(
                SourceKind::GitLab,
                "https://gitlab.example.com/group/repo/-/blob/main/memos.yaml"
            ),
            "https://gitlab.example.com/api/v4/projects/group%2Frepo/repository/files/memos.yaml/raw?ref=main"
        );
        assert_eq!(
            normalized_remote_url(
                SourceKind::GitLab,
                "https://gitlab.example.com/group/repo/-/raw/main/config/memos.yaml"
            ),
            "https://gitlab.example.com/api/v4/projects/group%2Frepo/repository/files/config%2Fmemos.yaml/raw?ref=main"
        );
    }

    #[test]
    fn resolves_configured_local_path() {
        let config = DataSourceConfig {
            kind: SourceKind::Local,
            local_path: " D:\\data\\memos.yaml ".to_owned(),
            ..DataSourceConfig::default()
        };
        let source = ContentSource::resolve(&config, Path::new("C:\\cache\\memos.yaml"));
        assert_eq!(
            source,
            ContentSource::Local(PathBuf::from("D:\\data\\memos.yaml"))
        );
    }
}
