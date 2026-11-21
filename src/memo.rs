//! 备忘录的 YAML 持久化。
//!
//! 文件只有一层 `memos` 列表，每项是一个纯文本字符串，方便直接用编辑器修改。

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use serde::Serialize;
use serde_yaml::Value;
use thiserror::Error;

const HEADER: &str = "\
# 快速粘贴 · 备忘录数据
# 纯文本 YAML，可直接编辑；回到程序后点击「重新加载」即可生效。
";

#[derive(Debug, Serialize)]
struct MemoDocument {
    memos: Vec<String>,
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
pub fn load_or_create(path: &Path) -> Result<Vec<String>, MemoError> {
    if !path.exists() {
        save(path, &[])?;
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)?;
    parse(&content)
}

/// 以原子方式写入备忘录文件。
pub fn save(path: &Path, memos: &[String]) -> Result<(), MemoError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let document = MemoDocument {
        memos: memos.to_vec(),
    };
    let yaml = serde_yaml::to_string(&document)?;
    let content = format!("{}{}", HEADER, yaml);

    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.commit()?;
    Ok(())
}

/// 解析 YAML。既接受 `memos:` 映射，也接受顶层的字符串列表。
pub fn parse(content: &str) -> Result<Vec<String>, MemoError> {
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
        match item {
            Value::String(text) => memos.push(text),
            Value::Number(number) => memos.push(number.to_string()),
            Value::Bool(flag) => memos.push(flag.to_string()),
            Value::Null => {}
            _ => {
                return Err(MemoError::Format(
                    "memos 的每一项必须是纯文本".to_owned(),
                ));
            }
        }
    }

    Ok(normalize(memos))
}

/// 按忽略大小写的子串匹配，返回原列表中的下标。
pub fn matching_indices<T: AsRef<str>>(memos: &[T], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..memos.len()).collect();
    }

    memos
        .iter()
        .enumerate()
        .filter_map(|(index, memo)| {
            let memo: &str = memo.as_ref();
            memo.to_lowercase().contains(&query).then_some(index)
        })
        .collect()
}

fn normalize(memos: Vec<String>) -> Vec<String> {
    memos
        .into_iter()
        .filter(|memo| !memo.trim().is_empty())
        .collect()
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

    #[test]
    fn parses_document_mapping() {
        let memos = parse(
            "\
memos:
- 第一条
- 第二条
",
        )
        .expect("should parse");
        assert_eq!(memos, vec!["第一条", "第二条"]);
    }

    #[test]
    fn parses_bare_sequence() {
        let memos = parse("- alpha\n- beta\n").expect("should parse");
        assert_eq!(memos, vec!["alpha", "beta"]);
    }

    #[test]
    fn parses_multiline_block_scalar() {
        let memos = parse(
            "\
memos:
- |-
  第一行
  第二行
",
        )
        .expect("should parse");
        assert_eq!(memos, vec!["第一行\n第二行"]);
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
        let memos = parse("- '   '\n- \"   缩进内容\"\n").expect("should parse");
        assert_eq!(memos, vec!["   缩进内容"]);
    }

    #[test]
    fn parses_numbers_and_booleans_as_text() {
        let memos = parse("memos:\n- 12345\n- 3.14\n- true\n").expect("should parse");
        assert_eq!(memos, vec!["12345", "3.14", "true"]);
    }

    #[test]
    fn saves_and_loads_round_trip() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("memos.yaml");
        let memos = vec![
            "第一行\n第二行".to_owned(),
            "邮箱：me@example.com".to_owned(),
            "  保留前导空格".to_owned(),
            "12345".to_owned(),
            "true".to_owned(),
            "null".to_owned(),
        ];

        save(&path, &memos).expect("save");
        let loaded = load_or_create(&path).expect("load");
        assert_eq!(loaded, memos);

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.starts_with("# 快速粘贴"));
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
    fn matching_indices_are_case_insensitive() {
        let memos = vec![
            "Hello World".to_owned(),
            "你好，世界".to_owned(),
            "hello again".to_owned(),
        ];
        assert_eq!(matching_indices(&memos, "HELLO"), vec![0, 2]);
        assert_eq!(matching_indices(&memos, "世界"), vec![1]);
        assert_eq!(matching_indices(&memos, "   "), vec![0, 1, 2]);
    }
}
