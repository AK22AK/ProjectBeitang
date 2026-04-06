use beitang::models::Priority;
use beitang::ui::parsing::{
    parse_person_list, parse_priority, parse_record_fields, parse_record_input, parse_tag_list,
    parse_tags_and_people, parse_task_input, reconcile_metadata,
};

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
    let result = parse_tags_and_people("Task # @john #work @");
    assert_eq!(result.0, "Task");
    assert_eq!(result.1, vec!["work"]);
    assert_eq!(result.2, vec!["john"]);
}

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

#[test]
fn test_parse_record_input() {
    let result = parse_record_input("Meeting notes #work @john #important");
    assert_eq!(result.0, "Meeting notes #work @john #important");
    assert_eq!(result.1, vec!["work", "important"]);
    assert_eq!(result.2, vec!["john"]);
}

#[test]
fn test_parse_record_input_preserves_line_breaks() {
    let result = parse_record_input("第一行内容\n#工作 @张三\n第二行内容");
    assert_eq!(result.0, "第一行内容\n#工作 @张三\n第二行内容");
    assert_eq!(result.1, vec!["工作"]);
    assert_eq!(result.2, vec!["张三"]);
}

#[test]
fn test_parse_record_fields_merges_and_dedups_metadata() {
    let result = parse_record_fields(
        Some("标题 #开发 @张三"),
        "正文内容\n#开发 @李四\n补充说明 @张三 #测试",
    );

    assert_eq!(result.title, Some("标题 #开发 @张三".to_string()));
    assert_eq!(
        result.content,
        "正文内容\n#开发 @李四\n补充说明 @张三 #测试"
    );
    assert_eq!(result.tags, vec!["开发", "测试"]);
    assert_eq!(result.people, vec!["张三", "李四"]);
}

#[test]
fn test_reconcile_metadata_preserves_non_inline_entries() {
    let result = reconcile_metadata(
        &["旧标签".to_string(), "正文标签".to_string()],
        &["正文标签".to_string()],
        &["新标签".to_string()],
    );

    assert_eq!(result, vec!["旧标签", "新标签"]);
}

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

#[test]
fn test_parse_person_list_comma_separated() {
    let result = parse_person_list("john, jane, bob");
    assert_eq!(result, vec!["john", "jane", "bob"]);
}

#[test]
fn test_parse_person_list_space_separated() {
    let result = parse_person_list("john jane bob");
    assert_eq!(result, vec!["john", "jane", "bob"]);
}
