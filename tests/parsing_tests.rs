use robinne::models::Priority;
use robinne::ui::parsing::{
    merge_inline_metadata, parse_line_ref, parse_person_list, parse_priority, parse_record_draft,
    parse_record_fields, parse_record_input, parse_tag_list, parse_tags_and_people,
    parse_task_draft, parse_task_input, reconcile_metadata,
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
fn test_parse_task_draft_single_line_keeps_empty_content() {
    let result = parse_task_draft("!! 任务标题 #开发 @张三");
    assert_eq!(result.title, "任务标题 #开发 @张三");
    assert_eq!(result.content, "");
    assert_eq!(result.priority, Priority::High);
    assert_eq!(result.tags, vec!["开发"]);
    assert_eq!(result.people, vec!["张三"]);
}

#[test]
fn test_parse_task_draft_multiline_splits_title_and_content() {
    let result = parse_task_draft("! 任务标题 #开发\n正文第一行 @张三\n正文第二行 #测试");
    assert_eq!(result.title, "任务标题 #开发");
    assert_eq!(result.content, "正文第一行 @张三\n正文第二行 #测试");
    assert_eq!(result.priority, Priority::Medium);
    assert_eq!(result.tags, vec!["开发", "测试"]);
    assert_eq!(result.people, vec!["张三"]);
}

#[test]
fn test_parse_record_draft_single_line_keeps_content_only() {
    let result = parse_record_draft("单行记录 #开发 @张三");
    assert_eq!(result.title, None);
    assert_eq!(result.content, "单行记录 #开发 @张三");
    assert_eq!(result.tags, vec!["开发"]);
    assert_eq!(result.people, vec!["张三"]);
}

#[test]
fn test_parse_record_draft_multiline_splits_title_and_content() {
    let result = parse_record_draft("记录标题 #开发\n正文第一行 @张三\n正文第二行 #测试");
    assert_eq!(result.title, Some("记录标题 #开发".to_string()));
    assert_eq!(result.content, "正文第一行 @张三\n正文第二行 #测试");
    assert_eq!(result.tags, vec!["开发", "测试"]);
    assert_eq!(result.people, vec!["张三"]);
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
fn test_parse_global_line_ref() {
    let line =
        parse_line_ref("补齐候选交互 ~快捷输入事务 #交互").expect("expected a valid global line");
    assert_eq!(line.project, None);
    assert_eq!(line.name, "快捷输入事务");
}

#[test]
fn test_parse_fullwidth_line_ref() {
    let line = parse_line_ref("～微信冻屏问题关键进展 #关键进展")
        .expect("expected a valid fullwidth line");
    assert_eq!(line.project, None);
    assert_eq!(line.name, "微信冻屏问题关键进展");
}

#[test]
fn test_parse_project_line_ref() {
    let line = parse_line_ref("补齐候选交互 ~Robinne/快捷输入事务 #交互")
        .expect("expected a valid project line");
    assert_eq!(line.project.as_deref(), Some("Robinne"));
    assert_eq!(line.name, "快捷输入事务");
}

#[test]
fn test_parse_line_ref_rejects_nested_project_paths() {
    assert!(parse_line_ref("记录 ~Robinne/输入/候选 #交互").is_none());
}

#[test]
fn test_parse_line_ref_rejects_empty_project_or_name() {
    assert!(parse_line_ref("记录 ~/候选").is_none());
    assert!(parse_line_ref("记录 ~Robinne/").is_none());
    assert!(parse_line_ref("记录 ~").is_none());
}

#[test]
fn test_parse_task_draft_extracts_line_without_removing_text() {
    let result = parse_task_draft("!! 补齐候选交互 ~Robinne/快捷输入事务 #交互 @自己");
    assert_eq!(
        result.title,
        "补齐候选交互 ~Robinne/快捷输入事务 #交互 @自己"
    );
    assert_eq!(result.tags, vec!["交互"]);
    assert_eq!(result.people, vec!["自己"]);
    let line = result.line.expect("expected task draft line");
    assert_eq!(line.project.as_deref(), Some("Robinne"));
    assert_eq!(line.name, "快捷输入事务");
}

#[test]
fn test_parse_record_fields_extracts_fullwidth_line() {
    let result = parse_record_fields(None, "～微信冻屏问题关键进展 #关键进展 刚用 glm 写了初稿");
    let line = result
        .line
        .expect("expected fullwidth line in record fields");
    assert_eq!(line.project, None);
    assert_eq!(line.name, "微信冻屏问题关键进展");
}

#[test]
fn test_parse_record_fields_extracts_people_adjacent_to_chinese_text() {
    let result = parse_record_fields(
        None,
        "~微信冻屏问题关键进展 #关键进展 刚用 glm 写了个初稿，发给了@沈慧海 @宋亚南",
    );
    assert_eq!(result.people, vec!["沈慧海", "宋亚南"]);
}

#[test]
fn test_parse_record_draft_extracts_line_from_title_and_body() {
    let result = parse_record_draft("事务设计 ~Robinne/事务\n补充节点展示 ~事务节点 #设计");
    assert_eq!(result.title, Some("事务设计 ~Robinne/事务".to_string()));
    assert_eq!(result.content, "补充节点展示 ~事务节点 #设计");
    let line = result.line.expect("expected first valid line");
    assert_eq!(line.project.as_deref(), Some("Robinne"));
    assert_eq!(line.name, "事务");
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
fn test_merge_inline_metadata_prefers_current_inline_order() {
    let result = merge_inline_metadata(
        &["宋亚南".to_string()],
        &["沈慧海".to_string(), "宋亚南".to_string()],
    );

    assert_eq!(result, vec!["沈慧海", "宋亚南"]);
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
