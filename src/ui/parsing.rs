use crate::models::Priority;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTaskDraft {
    pub title: String,
    pub content: String,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub people: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRecordDraft {
    pub title: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub people: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRecordFields {
    pub title: Option<String>,
    pub content: String,
    pub tags: Vec<String>,
    pub people: Vec<String>,
}

/// 解析任务输入，返回 (内容, 优先级, 标签列表, 人物列表)
pub fn parse_task_input(input: &str) -> (String, Priority, Vec<String>, Vec<String>) {
    let (content, priority) = parse_priority(input);
    let (tags, people) = extract_tags_and_people(&content);
    (content, priority, tags, people)
}

/// 解析记录输入，返回 (内容, 标签列表, 人物列表)
pub fn parse_record_input(input: &str) -> (String, Vec<String>, Vec<String>) {
    let content = input.trim().to_string();
    let (tags, people) = extract_tags_and_people(&content);
    (content, tags, people)
}

pub fn parse_record_fields(title: Option<&str>, content: &str) -> ParsedRecordFields {
    let normalized_title = normalize_optional_text(title.unwrap_or_default());
    let normalized_content = content.trim().to_string();
    let (title_tags, title_people) = normalized_title
        .as_deref()
        .map(extract_tags_and_people)
        .unwrap_or_else(|| (Vec::new(), Vec::new()));
    let (content_tags, content_people) = extract_tags_and_people(&normalized_content);

    ParsedRecordFields {
        title: normalized_title,
        content: normalized_content,
        tags: dedup_preserving_order(title_tags.into_iter().chain(content_tags)),
        people: dedup_preserving_order(title_people.into_iter().chain(content_people)),
    }
}

pub fn reconcile_metadata(
    existing: &[String],
    previous_inline: &[String],
    new_inline: &[String],
) -> Vec<String> {
    let previous_inline_set: HashSet<&str> =
        previous_inline.iter().map(|value| value.as_str()).collect();
    let preserved = existing
        .iter()
        .filter(|value| !previous_inline_set.contains(value.as_str()))
        .cloned();

    dedup_preserving_order(preserved.chain(new_inline.iter().cloned()))
}

pub fn parse_task_draft(input: &str) -> ParsedTaskDraft {
    let normalized = normalize_multiline_input(input);
    let Some(first_line) = normalized.first() else {
        return ParsedTaskDraft {
            title: String::new(),
            content: String::new(),
            priority: Priority::Low,
            tags: Vec::new(),
            people: Vec::new(),
        };
    };

    let (title, priority) = parse_priority(first_line);
    let content = normalized[1..].join("\n");
    let inline_fields = parse_record_fields(Some(&title), &content);

    ParsedTaskDraft {
        title,
        content,
        priority,
        tags: inline_fields.tags,
        people: inline_fields.people,
    }
}

pub fn parse_record_draft(input: &str) -> ParsedRecordDraft {
    let normalized = normalize_multiline_input(input);
    match normalized.as_slice() {
        [] => ParsedRecordDraft {
            title: None,
            content: String::new(),
            tags: Vec::new(),
            people: Vec::new(),
        },
        [single] => {
            let content = single.to_string();
            let (tags, people) = extract_tags_and_people(&content);
            ParsedRecordDraft {
                title: None,
                content,
                tags,
                people,
            }
        }
        [title, rest @ ..] => {
            let content = rest.join("\n");
            let inline_fields = parse_record_fields(Some(title), &content);
            ParsedRecordDraft {
                title: inline_fields.title,
                content: inline_fields.content,
                tags: inline_fields.tags,
                people: inline_fields.people,
            }
        }
    }
}

/// 解析优先级，返回 (去除优先级的内容, 优先级)
///
/// 支持的语法：
/// - `!!` 或 `！！` → High 优先级
/// - `!` 或 `！` → Medium 优先级
/// - 无标记 → Low 优先级
pub fn parse_priority(input: &str) -> (String, Priority) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed
        .strip_prefix("!!")
        .or_else(|| trimmed.strip_prefix("！！"))
    {
        (rest.trim_start().to_string(), Priority::High)
    } else if let Some(rest) = trimmed
        .strip_prefix("!")
        .or_else(|| trimmed.strip_prefix("！"))
    {
        (rest.trim_start().to_string(), Priority::Medium)
    } else {
        (trimmed.to_string(), Priority::Low)
    }
}

/// 解析标签和人物，返回 (纯内容, 标签列表, 人物列表)
///
/// 支持的语法：
/// - `#标签名` → 标签（不含#）
/// - `@人物名` → 人物（不含@）
///
/// 标签和人物可以出现在输入的任何位置，会被提取出来，
/// 剩余的内容作为纯内容返回。
pub fn parse_tags_and_people(input: &str) -> (String, Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut people = Vec::new();
    let mut cleaned_lines = Vec::new();

    for line in input.lines() {
        let mut content_parts = Vec::new();

        for word in line.split_whitespace() {
            if let Some(tag) = word.strip_prefix('#') {
                if !tag.is_empty() {
                    tags.push(tag.to_string());
                }
            } else if let Some(person) = word.strip_prefix('@') {
                if !person.is_empty() {
                    people.push(person.to_string());
                }
            } else {
                content_parts.push(word);
            }
        }

        cleaned_lines.push(content_parts.join(" "));
    }

    let content = trim_empty_edge_lines(cleaned_lines).join("\n");
    (content.trim().to_string(), tags, people)
}

pub fn extract_tags_and_people(input: &str) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut people = Vec::new();

    for word in input.split_whitespace() {
        if let Some(tag) = word.strip_prefix('#') {
            if !tag.is_empty() {
                tags.push(tag.to_string());
            }
        } else if let Some(person) = word.strip_prefix('@') {
            if !person.is_empty() {
                people.push(person.to_string());
            }
        }
    }

    (dedup_preserving_order(tags), dedup_preserving_order(people))
}

/// 批量解析多个标签（从逗号或空白分隔的字符串）
pub fn parse_tag_list(input: &str) -> Vec<String> {
    input
        .split(&[',', '，', ' ', '\t', '\n'][..])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// 批量解析多个人物（从逗号或空白分隔的字符串）
pub fn parse_person_list(input: &str) -> Vec<String> {
    input
        .split(&[',', '，', ' ', '\t', '\n'][..])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn normalize_optional_text(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn dedup_preserving_order<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for value in values {
        if seen.insert(value.clone()) {
            result.push(value);
        }
    }

    result
}

fn normalize_multiline_input(input: &str) -> Vec<String> {
    trim_empty_edge_lines(input.lines().map(|line| line.to_string()).collect())
}

fn trim_empty_edge_lines(lines: Vec<String>) -> Vec<String> {
    let start = lines.iter().position(|line| !line.trim().is_empty());
    let end = lines.iter().rposition(|line| !line.trim().is_empty());

    match (start, end) {
        (Some(start), Some(end)) if start <= end => lines[start..=end].to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== 优先级解析测试 ==========

    #[test]
    fn test_parse_priority_high_with_space() {
        let result = parse_priority("!! High priority task");
        assert_eq!(result.0, "High priority task");
        assert_eq!(result.1, Priority::High);
    }

    #[test]
    fn test_parse_priority_high_without_space() {
        let result = parse_priority("!!High priority task");
        assert_eq!(result.0, "High priority task");
        assert_eq!(result.1, Priority::High);
    }

    #[test]
    fn test_parse_priority_high_chinese_exclamation() {
        let result = parse_priority("！！Chinese exclamation");
        assert_eq!(result.0, "Chinese exclamation");
        assert_eq!(result.1, Priority::High);
    }

    #[test]
    fn test_parse_priority_medium_with_space() {
        let result = parse_priority("! Medium priority task");
        assert_eq!(result.0, "Medium priority task");
        assert_eq!(result.1, Priority::Medium);
    }

    #[test]
    fn test_parse_priority_medium_without_space() {
        let result = parse_priority("!Medium priority task");
        assert_eq!(result.0, "Medium priority task");
        assert_eq!(result.1, Priority::Medium);
    }

    #[test]
    fn test_parse_priority_low() {
        let result = parse_priority("Just a normal task");
        assert_eq!(result.0, "Just a normal task");
        assert_eq!(result.1, Priority::Low);
    }

    #[test]
    fn test_parse_priority_trims_whitespace() {
        let result = parse_priority("  !!  Task with extra spaces  ");
        assert_eq!(result.0, "Task with extra spaces");
        assert_eq!(result.1, Priority::High);
    }

    #[test]
    fn test_parse_priority_empty_after_priority() {
        let result = parse_priority("!!");
        assert_eq!(result.0, "");
        assert_eq!(result.1, Priority::High);
    }

    // ========== 标签和人物解析测试 ==========

    #[test]
    fn test_parse_single_tag() {
        let result = parse_tags_and_people("Task content #work");
        assert_eq!(result.0, "Task content");
        assert_eq!(result.1, vec!["work"]);
        assert_eq!(result.2, Vec::<String>::new());
    }

    #[test]
    fn test_parse_multiple_tags() {
        let result = parse_tags_and_people("Task #work #urgent #project");
        assert_eq!(result.0, "Task");
        assert_eq!(result.1, vec!["work", "urgent", "project"]);
        assert_eq!(result.2, Vec::<String>::new());
    }

    #[test]
    fn test_parse_single_person() {
        let result = parse_tags_and_people("Meeting with @john");
        assert_eq!(result.0, "Meeting with");
        assert_eq!(result.1, Vec::<String>::new());
        assert_eq!(result.2, vec!["john"]);
    }

    #[test]
    fn test_parse_multiple_persons() {
        let result = parse_tags_and_people("Meeting @john @jane @bob");
        assert_eq!(result.0, "Meeting");
        assert_eq!(result.1, Vec::<String>::new());
        assert_eq!(result.2, vec!["john", "jane", "bob"]);
    }

    #[test]
    fn test_parse_tags_and_persons_together() {
        let result = parse_tags_and_people("Task #work @john #urgent @jane");
        assert_eq!(result.0, "Task");
        assert_eq!(result.1, vec!["work", "urgent"]);
        assert_eq!(result.2, vec!["john", "jane"]);
    }

    #[test]
    fn test_parse_empty_content() {
        let result = parse_tags_and_people("#work @john");
        assert_eq!(result.0, "");
        assert_eq!(result.1, vec!["work"]);
        assert_eq!(result.2, vec!["john"]);
    }

    #[test]
    fn test_parse_only_content() {
        let result = parse_tags_and_people("Just plain content");
        assert_eq!(result.0, "Just plain content");
        assert_eq!(result.1, Vec::<String>::new());
        assert_eq!(result.2, Vec::<String>::new());
    }

    #[test]
    fn test_parse_unicode_tags() {
        let result = parse_tags_and_people("任务 #工作 #紧急 @张三");
        assert_eq!(result.0, "任务");
        assert_eq!(result.1, vec!["工作", "紧急"]);
        assert_eq!(result.2, vec!["张三"]);
    }

    #[test]
    fn test_parse_empty_tag_or_person() {
        // 单独的 # 或 @ 应该被忽略
        let result = parse_tags_and_people("Task # @john #work @");
        assert_eq!(result.0, "Task");
        assert_eq!(result.1, vec!["work"]);
        assert_eq!(result.2, vec!["john"]);
    }

    // ========== 任务输入完整解析测试 ==========

    #[test]
    fn test_parse_task_input_full() {
        let result = parse_task_input("!! High priority task #work @john #urgent");
        assert_eq!(result.0, "High priority task #work @john #urgent");
        assert_eq!(result.1, Priority::High);
        assert_eq!(result.2, vec!["work", "urgent"]);
        assert_eq!(result.3, vec!["john"]);
    }

    #[test]
    fn test_parse_task_input_no_tags() {
        let result = parse_task_input("! Medium priority task");
        assert_eq!(result.0, "Medium priority task");
        assert_eq!(result.1, Priority::Medium);
        assert_eq!(result.2, Vec::<String>::new());
        assert_eq!(result.3, Vec::<String>::new());
    }

    #[test]
    fn test_parse_task_input_only_tags() {
        let result = parse_task_input("!! #work @john");
        assert_eq!(result.0, "#work @john");
        assert_eq!(result.1, Priority::High);
        assert_eq!(result.2, vec!["work"]);
        assert_eq!(result.3, vec!["john"]);
    }

    // ========== 记录输入解析测试 ==========

    #[test]
    fn test_parse_record_input() {
        let result = parse_record_input("Meeting notes #work @john #important");
        assert_eq!(result.0, "Meeting notes #work @john #important");
        assert_eq!(result.1, vec!["work", "important"]);
        assert_eq!(result.2, vec!["john"]);
    }

    // ========== 标签列表解析测试 ==========

    #[test]
    fn test_parse_tag_list_comma_separated() {
        let result = parse_tag_list("work, urgent, project");
        assert_eq!(result, vec!["work", "urgent", "project"]);
    }

    #[test]
    fn test_parse_tag_list_chinese_comma() {
        let result = parse_tag_list("工作，紧急，项目");
        assert_eq!(result, vec!["工作", "紧急", "项目"]);
    }

    #[test]
    fn test_parse_tag_list_space_separated() {
        let result = parse_tag_list("work urgent project");
        assert_eq!(result, vec!["work", "urgent", "project"]);
    }

    #[test]
    fn test_parse_tag_list_mixed() {
        let result = parse_tag_list("work, urgent  project");
        assert_eq!(result, vec!["work", "urgent", "project"]);
    }

    #[test]
    fn test_parse_tag_list_empty() {
        let result = parse_tag_list("");
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_parse_tag_list_with_empty_items() {
        let result = parse_tag_list("work,, urgent, , project");
        assert_eq!(result, vec!["work", "urgent", "project"]);
    }
}
