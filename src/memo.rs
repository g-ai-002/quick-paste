//! 备忘录的 YAML 持久化。
//!
//! 每条记录包含 `text`、`description`、`usage_count`、`is_pinned`、`tags`
//! 五个字段。`text` / `description` / `tags` 以外部编辑的 `memos.yaml` 为准，
//! `usage_count` / `is_pinned` 由本程序维护；重新加载时优先沿用本地文件里的
//! 这两个值。

use std::{
    collections::HashSet,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use thiserror::Error;

const HEADER: &str = "\
# 备忘快贴 · 备忘录数据
# text / description / tags 由外部编辑；usage_count / is_pinned 由本程序维护。
# 修改后回到程序点击「重新加载」即可生效。
";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memo {
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub usage_count: u64,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MemoDocument<'a> {
    memos: &'a [Memo],
}

#[derive(Debug, Error)]
pub enum MemoError {
    #[error("读写备忘录文件失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("备忘录 YAML 格式无效：{0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("备忘录格式无效：{0}")]
    Format(String),
}

/// 读取备忘录；文件不存在时创建一份空列表并返回空列表。
pub fn load_or_create(path: &Path) -> Result<Vec<Memo>, MemoError> {
    if !path.exists() {
        save(path, &[])?;
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)?;
    parse(&content)
}

/// 读取已存在的备忘录文件，不会创建新文件。
pub fn load(path: &Path) -> Result<Vec<Memo>, MemoError> {
    let content = fs::read_to_string(path)?;
    parse(&content)
}

/// 以原子方式写入备忘录文件。
pub fn save(path: &Path, memos: &[Memo]) -> Result<(), MemoError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let document = MemoDocument { memos };
    let yaml = serde_yaml::to_string(&document)?;
    let content = format!("{}{}", HEADER, yaml);

    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.commit()?;
    Ok(())
}

/// 解析 YAML。既接受 `memos:` 映射，也接受顶层的记录列表。
pub fn parse(content: &str) -> Result<Vec<Memo>, MemoError> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let only_comments = content
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'));
    if only_comments {
        return Ok(Vec::new());
    }

    let value: Value = serde_yaml::from_str(content)?;
    let items = match value {
        Value::Mapping(map) => map
            .into_iter()
            .find_map(|(key, value)| match key {
                Value::String(key) if key == "memos" => Some(value),
                _ => None,
            })
            .unwrap_or(Value::Sequence(Vec::new())),
        Value::Sequence(sequence) => Value::Sequence(sequence),
        Value::Null => Value::Sequence(Vec::new()),
        _ => {
            return Err(MemoError::Format(
                "顶层必须是 memos 列表".to_owned(),
            ));
        }
    };

    let items = match items {
        Value::Sequence(items) => items,
        Value::Null => Vec::new(),
        _ => return Err(MemoError::Format("memos 必须是列表".to_owned())),
    };

    let mut memos = Vec::with_capacity(items.len());
    for item in items {
        memos.push(parse_item(item)?);
    }

    Ok(normalize(memos))
}

fn parse_item(item: Value) -> Result<Memo, MemoError> {
    match item {
        item @ Value::Mapping(_) => serde_yaml::from_value(item)
            .map_err(|error| MemoError::Format(format!("备忘录字段无效：{error}"))),
        _ => Err(MemoError::Format(
            "memos 的每一项必须是记录映射".to_owned(),
        )),
    }
}

/// 列表顺序：置顶优先，其次按使用次数从多到少；次数相同保持文件顺序。
pub fn sort_for_display(memos: &mut [Memo]) {
    memos.sort_by_key(|memo| (!memo.is_pinned, std::cmp::Reverse(memo.usage_count)));
}

/// 以 `source` 的 text / description / tags 为准，优先沿用 `local` 的
/// usage_count / is_pinned。
pub fn merge(source: &[Memo], local: &[Memo]) -> Vec<Memo> {
    source
        .iter()
        .map(|memo| {
            let state = local.iter().find(|local| local.text == memo.text);
            Memo {
                text: memo.text.clone(),
                description: memo.description.clone(),
                usage_count: state.map_or(memo.usage_count, |state| state.usage_count),
                is_pinned: state.map_or(memo.is_pinned, |state| state.is_pinned),
                tags: memo.tags.clone(),
            }
        })
        .collect()
}

/// 按忽略大小写的子串匹配文本、备注或标签，返回原列表中的下标。
pub fn matching_indices(memos: &[Memo], query: &str) -> Vec<usize> {
    matching_indices_with_tag(memos, query, None)
}

/// 在搜索词之外再按标签精确过滤；`tag` 为 `None` 表示不限标签。
/// 标签比较忽略大小写，搜索词仍按子串匹配文本、备注或标签。
pub fn matching_indices_with_tag(
    memos: &[Memo],
    query: &str,
    tag: Option<&str>,
) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let tag = tag.map(str::to_lowercase);

    memos
        .iter()
        .enumerate()
        .filter_map(|(index, memo)| {
            let matches_tag = tag.as_ref().is_none_or(|tag| {
                memo.tags
                    .iter()
                    .any(|item| item.to_lowercase() == *tag)
            });
            if !matches_tag {
                return None;
            }

            let matches_query = query.is_empty()
                || memo.text.to_lowercase().contains(&query)
                || memo.description.to_lowercase().contains(&query)
                || memo
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query));
            matches_query.then_some(index)
        })
        .collect()
}

/// 收集所有备忘录的标签，忽略大小写去重后按字典序排列。
pub fn sorted_tags(memos: &[Memo]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags: Vec<String> = memos
        .iter()
        .flat_map(|memo| memo.tags.iter())
        .filter(|tag| seen.insert(tag.to_lowercase()))
        .cloned()
        .collect();
    tags.sort_by_cached_key(|tag| (tag.to_lowercase(), tag.clone()));
    tags
}

/// 把卡片正文的可用宽度换算成“半角字符单位”。
///
/// 正文使用 14px 字号，半角字符平均约 6.4px；中文等全角字符约 2.3 个单位。
pub fn line_units_for_width(width: f32) -> usize {
    const HALF_WIDTH_PX: f32 = 6.4;

    ((width.max(1.0) / HALF_WIDTH_PX).floor() as usize).max(1)
}

/// 列表里需要展示的文本行数：按显式换行和估算的自动换行统计，
/// 并限制在 `1..=max`。
pub fn display_line_count(text: &str, max: usize, line_units: usize) -> usize {
    wrapped_line_count(text, line_units).clamp(1, max.max(1))
}

/// 文本按卡片宽度估算后是否超过 `max` 行，超过即需要悬停预览。
pub fn is_truncated(text: &str, max: usize, line_units: usize) -> bool {
    wrapped_line_count(text, line_units) > max.max(1)
}

fn wrapped_line_count(text: &str, line_units: usize) -> usize {
    // 空行也占一行。
    let line_units = line_units.max(1) as f32;

    text.lines()
        .map(|line| {
            let units: f32 = line.chars().map(char_units).sum();
            (units.max(1.0) / line_units).ceil() as usize
        })
        .sum()
}

/// 单个字符的估算宽度，单位与 `line_units_for_width` 一致。
///
/// 按 14px 字号下 Segoe UI 的实际字宽分档，避免统一按 7.2px 估算时
/// 把长 ASCII 命令多算出一行。
fn char_units(ch: char) -> f32 {
    if !ch.is_ascii() {
        return 2.3;
    }

    match ch {
        ' ' => 0.66,
        'i' | 'l' | 'j' | 't' | 'f' | 'r' | 'I' | '!' | '.' | ',' | ':' | ';' | '|' | '\'' => 0.63,
        'm' | 'w' | 'M' | 'W' => 2.0,
        'A'..='Z' => 1.34,
        '0'..='9' => 1.18,
        'a'..='z' => 1.08,
        _ => 0.98,
    }
}

fn normalize(memos: Vec<Memo>) -> Vec<Memo> {
    memos.into_iter().filter_map(normalize_one).collect()
}

fn normalize_one(mut memo: Memo) -> Option<Memo> {
    if memo.text.trim().is_empty() {
        return None;
    }

    memo.description = memo.description.trim().to_owned();

    let mut tags = Vec::with_capacity(memo.tags.len());
    let mut seen = HashSet::new();
    for tag in &memo.tags {
        let tag = tag.trim();
        if !tag.is_empty() && seen.insert(tag.to_lowercase()) {
            tags.push(tag.to_owned());
        }
    }
    memo.tags = tags;
    Some(memo)
}

/// 外部编辑的数据文件：优先当前工作目录，其次可执行文件所在目录。
pub fn external_source_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(directory) = std::env::current_dir() {
        candidates.push(directory.join("memos.yaml"));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        candidates.push(directory.join("memos.yaml"));
    }
    candidates.into_iter().find(|path| path.exists())
}

/// 在文件管理器中选中数据文件；文件不存在时打开所在目录。
#[cfg(target_os = "windows")]
pub fn reveal(path: &Path) -> std::io::Result<()> {
    use std::process::Command;

    let mut command = Command::new("explorer.exe");
    if path.exists() {
        command.arg("/select,").arg(path);
    } else if let Some(parent) = path.parent() {
        command.arg(parent);
    }
    command.spawn().map(|_| ())
}

#[cfg(not(target_os = "windows"))]
pub fn reveal(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// 数据目录：`%LOCALAPPDATA%\QuickPaste`。
pub fn data_directory() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join("QuickPaste")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn described_memo(
        text: &str,
        description: &str,
        usage_count: u64,
        is_pinned: bool,
        tags: &[&str],
    ) -> Memo {
        Memo {
            text: text.to_owned(),
            description: description.to_owned(),
            usage_count,
            is_pinned,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        }
    }

    fn memo(text: &str, usage_count: u64, is_pinned: bool, tags: &[&str]) -> Memo {
        described_memo(text, "", usage_count, is_pinned, tags)
    }

    #[test]
    fn parses_document_mapping() {
        let memos = parse(
            "\
memos:
- text: 第一条
  description: 示例备注
  usage_count: 3
  is_pinned: true
  tags:
    - 工作
    - 常用
- text: 第二条
",
        )
        .expect("should parse");
        assert_eq!(memos.len(), 2);
        assert_eq!(
            memos[0],
            described_memo("第一条", "示例备注", 3, true, &["工作", "常用"])
        );
        assert_eq!(memos[1], memo("第二条", 0, false, &[]));
    }

    #[test]
    fn parses_bare_sequence_of_records() {
        let memos = parse("- text: alpha\n- text: beta\n").expect("should parse");
        assert_eq!(
            memos,
            vec![memo("alpha", 0, false, &[]), memo("beta", 0, false, &[])]
        );
    }

    #[test]
    fn parses_multiline_block_scalar() {
        let memos = parse(
            "\
memos:
- text: |-
    第一行
    第二行
",
        )
        .expect("should parse");
        assert_eq!(memos, vec![memo("第一行\n第二行", 0, false, &[])]);
    }

    #[test]
    fn parses_comment_only_file_as_empty() {
        let memos = parse("# 只有注释\n\n   \n").expect("should parse");
        assert!(memos.is_empty());
    }

    #[test]
    fn parses_empty_memos_key_as_empty_list() {
        let memos = parse("memos:\n").expect("should parse");
        assert!(memos.is_empty());
    }

    #[test]
    fn drops_blank_entries_but_keeps_leading_spaces() {
        let memos =
            parse("memos:\n- text: '   '\n- text: \"   缩进内容\"\n").expect("should parse");
        assert_eq!(memos, vec![memo("   缩进内容", 0, false, &[])]);
    }

    #[test]
    fn rejects_legacy_string_entries() {
        let error = parse("memos:\n- 纯文本\n").expect_err("should reject");
        assert!(matches!(error, MemoError::Format(_)));
    }

    #[test]
    fn normalizes_tags() {
        let memos = parse(
            "\
memos:
- text: 备忘
  tags:
    - 工作
    - ' 工作 '
    - ''
    - AI
    - ai
",
        )
        .expect("should parse");
        assert_eq!(memos[0].tags, vec!["工作", "AI"]);
    }

    #[test]
    fn trims_description() {
        let memos = parse(
            "\
memos:
- text: 备忘
  description: '  备注内容  '
",
        )
        .expect("should parse");
        assert_eq!(memos[0].description, "备注内容");
    }

    #[test]
    fn sorts_pinned_first_then_usage() {
        let mut memos = vec![
            memo("普通一", 1, false, &[]),
            memo("置顶一", 1, true, &[]),
            memo("普通二", 5, false, &[]),
            memo("置顶二", 5, true, &[]),
        ];

        sort_for_display(&mut memos);

        let texts: Vec<&str> = memos.iter().map(|memo| memo.text.as_str()).collect();
        assert_eq!(texts, ["置顶二", "置顶一", "普通二", "普通一"]);
    }

    #[test]
    fn keeps_file_order_for_equal_usage() {
        let mut memos = vec![memo("先", 2, false, &[]), memo("后", 2, false, &[])];
        sort_for_display(&mut memos);
        let texts: Vec<&str> = memos.iter().map(|memo| memo.text.as_str()).collect();
        assert_eq!(texts, ["先", "后"]);
    }

    #[test]
    fn merge_keeps_local_usage_and_pin() {
        let source = vec![
            described_memo("甲", "新备注", 1, false, &["新标签"]),
            memo("乙", 2, true, &[]),
        ];
        let local = vec![described_memo("甲", "旧备注", 9, true, &["旧标签"])];

        let merged = merge(&source, &local);

        assert_eq!(
            merged[0],
            described_memo("甲", "新备注", 9, true, &["新标签"])
        );
        assert_eq!(merged[1], memo("乙", 2, true, &[]));
    }

    #[test]
    fn saves_and_loads_round_trip() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("memos.yaml");
        let memos = vec![
            memo("第一行\n第二行", 2, true, &["工作", "常用"]),
            described_memo("带备注", "这是一条备注", 0, false, &[]),
            memo("邮箱：me@example.com", 0, false, &[]),
            memo("  保留前导空格", 5, false, &["tag"]),
            memo("12345", 0, false, &[]),
            memo("true", 0, false, &[]),
            memo("null", 0, false, &[]),
        ];

        save(&path, &memos).expect("save");
        let loaded = load_or_create(&path).expect("load");
        assert_eq!(loaded, memos);

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.starts_with("# 备忘快贴"));
        assert!(content.contains("这是一条备注"));
    }

    #[test]
    fn creates_missing_file_as_empty_list() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("nested").join("memos.yaml");

        let memos = load_or_create(&path).expect("load");
        assert!(memos.is_empty());
        assert!(path.exists());

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("memos: []"));
    }

    #[test]
    fn matching_indices_match_text_description_and_tags() {
        let memos = vec![
            memo("Hello World", 0, false, &["工作"]),
            described_memo("你好，世界", "问候语", 0, false, &[]),
            memo("hello again", 0, false, &["AI"]),
        ];
        assert_eq!(matching_indices(&memos, "HELLO"), vec![0, 2]);
        assert_eq!(matching_indices(&memos, "世界"), vec![1]);
        assert_eq!(matching_indices(&memos, "问候"), vec![1]);
        assert_eq!(matching_indices(&memos, "ai"), vec![2]);
        assert_eq!(matching_indices(&memos, "   "), vec![0, 1, 2]);
    }

    #[test]
    fn filters_by_exact_tag_and_query() {
        let memos = vec![
            memo("第一条", 0, false, &["工作", "常用"]),
            memo("第二条", 0, false, &["工作"]),
            memo("第三条", 0, false, &["生活"]),
            memo("第四条", 0, false, &["AI"]),
        ];

        assert_eq!(
            matching_indices_with_tag(&memos, "", Some("工作")),
            vec![0, 1]
        );
        assert_eq!(
            matching_indices_with_tag(&memos, "第二条", Some("工作")),
            vec![1]
        );
        assert_eq!(
            matching_indices_with_tag(&memos, "", Some("作")),
            Vec::<usize>::new()
        );
        assert_eq!(
            matching_indices_with_tag(&memos, "", Some("常用")),
            vec![0]
        );
        assert_eq!(
            matching_indices_with_tag(&memos, "", Some("ai")),
            vec![3]
        );
        assert_eq!(
            matching_indices_with_tag(&memos, "", None),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn collects_tags_case_insensitively_in_dictionary_order() {
        let memos = vec![
            memo("一", 0, false, &["beta", "工作"]),
            memo("二", 0, false, &["Alpha", "beta"]),
            memo("三", 0, false, &["alpha"]),
        ];

        assert_eq!(sorted_tags(&memos), vec!["Alpha", "beta", "工作"]);
    }

    #[test]
    fn display_line_count_accounts_for_wrapping_and_clamps() {
        assert_eq!(display_line_count("", 3, 48), 1);
        assert_eq!(display_line_count("一行", 3, 48), 1);
        assert_eq!(display_line_count("一行\n两行", 3, 48), 2);
        assert_eq!(display_line_count("一\n二\n三\n四", 3, 48), 3);
        assert_eq!(display_line_count("一\n\n三", 3, 48), 3);

        assert_eq!(display_line_count(&"a".repeat(44), 3, 48), 1);
        assert_eq!(display_line_count(&"a".repeat(45), 3, 48), 2);
        assert_eq!(display_line_count(&"中".repeat(20), 3, 48), 1);
        assert_eq!(display_line_count(&"中".repeat(21), 3, 48), 2);
        assert_eq!(display_line_count(&"中".repeat(200), 3, 48), 3);
    }

    #[test]
    fn converts_card_width_to_half_width_units() {
        assert_eq!(line_units_for_width(0.0), 1);
        assert_eq!(line_units_for_width(536.0), 83);
        assert_eq!(
            display_line_count(&"a".repeat(70), 3, line_units_for_width(536.0)),
            1
        );
        assert_eq!(
            display_line_count(&"a".repeat(80), 3, line_units_for_width(536.0)),
            2
        );
    }

    #[test]
    fn long_ascii_command_does_not_reserve_an_extra_line() {
        let text = r#"find . -maxdepth 1 -type f -name "*.yaml" -exec cat {} >> result.txt \; -exec echo >> result.txt \; -exec echo '---' >> result.txt \; -exec echo >> result.txt \;"#;
        let units = line_units_for_width(536.0);
        assert_eq!(display_line_count(text, 3, units), 2);
        assert!(is_truncated(text, 1, units));
    }

    #[test]
    fn detects_text_that_does_not_fit_the_card() {
        assert!(!is_truncated("短文本", 3, 48));
        assert!(!is_truncated("一\n二\n三", 3, 48));
        assert!(is_truncated("一\n二\n三\n四", 3, 48));
        assert!(is_truncated(&"a".repeat(45), 1, 48));
        assert!(!is_truncated(&"a".repeat(44), 1, 48));
        assert!(!is_truncated(
            &"a".repeat(70),
            3,
            line_units_for_width(536.0)
        ));
    }
}
