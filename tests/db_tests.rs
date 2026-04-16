use chrono::{Duration, Utc};
use robinne::db::Database;
use robinne::models::{Priority, Record, RecordType, TimelineQuery};
use tempfile::TempDir;

fn setup_test_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    (db, temp_dir)
}

fn titles(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.display_title())
        .collect()
}

#[test]
fn test_create_and_get_task() {
    let (db, _temp) = setup_test_db();

    let task = Record::new_task(
        "Test Title".to_string(),
        "Test content".to_string(),
        Priority::High,
    );
    db.create_record(&task).unwrap();

    let tasks = db.get_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, Some("Test Title".to_string()));
    assert_eq!(tasks[0].content, "Test content");
    assert_eq!(tasks[0].priority, Some(Priority::High));
    assert_eq!(tasks[0].record_type, RecordType::Task);
}

#[test]
fn test_get_tasks_returns_empty_when_no_tasks() {
    let (db, _temp) = setup_test_db();

    let tasks = db.get_tasks().unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_create_multiple_tasks() {
    let (db, _temp) = setup_test_db();

    let task1 = Record::new_task("Task 1".to_string(), "".to_string(), Priority::High);
    let task2 = Record::new_task("Task 2".to_string(), "".to_string(), Priority::Medium);
    let task3 = Record::new_task("Task 3".to_string(), "".to_string(), Priority::Low);

    db.create_record(&task1).unwrap();
    db.create_record(&task2).unwrap();
    db.create_record(&task3).unwrap();

    let tasks = db.get_tasks().unwrap();
    assert_eq!(tasks.len(), 3);
}

#[test]
fn test_task_priorities_preserved_correctly() {
    let (db, _temp) = setup_test_db();

    // Create tasks with different priorities
    let high = Record::new_task("High".to_string(), "".to_string(), Priority::High);
    let medium = Record::new_task("Medium".to_string(), "".to_string(), Priority::Medium);
    let low = Record::new_task("Low".to_string(), "".to_string(), Priority::Low);

    db.create_record(&high).unwrap();
    db.create_record(&medium).unwrap();
    db.create_record(&low).unwrap();

    let tasks = db.get_tasks().unwrap();

    let high_task = tasks
        .iter()
        .find(|t| t.title == Some("High".to_string()))
        .unwrap();
    let medium_task = tasks
        .iter()
        .find(|t| t.title == Some("Medium".to_string()))
        .unwrap();
    let low_task = tasks
        .iter()
        .find(|t| t.title == Some("Low".to_string()))
        .unwrap();

    assert_eq!(high_task.priority, Some(Priority::High));
    assert_eq!(medium_task.priority, Some(Priority::Medium));
    assert_eq!(low_task.priority, Some(Priority::Low));
}

#[test]
fn test_update_existing_task() {
    let (db, _temp) = setup_test_db();

    let mut task = Record::new_task("Original".to_string(), "".to_string(), Priority::Low);
    db.create_record(&task).unwrap();

    // Modify and update
    task.content = "Updated content".to_string();
    task.priority = Some(Priority::High);
    db.create_record(&task).unwrap();

    let tasks = db.get_tasks().unwrap();
    assert_eq!(tasks.len(), 1); // Still one task
    assert_eq!(tasks[0].content, "Updated content");
    assert_eq!(tasks[0].priority, Some(Priority::High));
}

#[test]
fn test_complete_task() {
    let (db, _temp) = setup_test_db();

    let mut task = Record::new_task("To complete".to_string(), "".to_string(), Priority::Medium);
    db.create_record(&task).unwrap();

    assert!(!task.is_completed());

    task.complete();
    assert!(task.is_completed());

    // Save completed task
    db.create_record(&task).unwrap();

    // Reload and verify
    let tasks = db.get_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].is_completed());
    assert!(tasks[0].completed_at.is_some());
}

#[test]
fn test_timeline_query_filters_by_tags_with_and_semantics() {
    let (db, _temp) = setup_test_db();

    let mut both = Record::new_note("同时命中".to_string());
    both.tags = vec!["工作".to_string(), "紧急".to_string()];

    let mut only_work = Record::new_note("只命中工作".to_string());
    only_work.tags = vec!["工作".to_string()];

    let mut only_urgent = Record::new_note("只命中紧急".to_string());
    only_urgent.tags = vec!["紧急".to_string()];

    db.create_record(&both).unwrap();
    db.create_record(&only_work).unwrap();
    db.create_record(&only_urgent).unwrap();

    let results = db
        .get_timeline(&TimelineQuery {
            limit: 20,
            offset: 0,
            tags: vec!["工作".to_string(), "紧急".to_string()],
            persons: Vec::new(),
        })
        .unwrap();

    assert_eq!(titles(&results), vec!["同时命中".to_string()]);
}

#[test]
fn test_timeline_query_filters_by_persons_with_and_semantics() {
    let (db, _temp) = setup_test_db();

    let mut both = Record::new_note("同时命中人物".to_string());
    both.persons = vec!["张三".to_string(), "李四".to_string()];

    let mut only_zhang = Record::new_note("只命中张三".to_string());
    only_zhang.persons = vec!["张三".to_string()];

    db.create_record(&both).unwrap();
    db.create_record(&only_zhang).unwrap();

    let results = db
        .get_timeline(&TimelineQuery {
            limit: 20,
            offset: 0,
            tags: Vec::new(),
            persons: vec!["张三".to_string(), "李四".to_string()],
        })
        .unwrap();

    assert_eq!(titles(&results), vec!["同时命中人物".to_string()]);
}

#[test]
fn test_timeline_query_filters_by_tags_and_persons_together() {
    let (db, _temp) = setup_test_db();

    let mut matched = Record::new_note("标签人物都命中".to_string());
    matched.tags = vec!["工作".to_string()];
    matched.persons = vec!["张三".to_string()];

    let mut wrong_tag = Record::new_note("标签不命中".to_string());
    wrong_tag.tags = vec!["生活".to_string()];
    wrong_tag.persons = vec!["张三".to_string()];

    let mut wrong_person = Record::new_note("人物不命中".to_string());
    wrong_person.tags = vec!["工作".to_string()];
    wrong_person.persons = vec!["李四".to_string()];

    db.create_record(&matched).unwrap();
    db.create_record(&wrong_tag).unwrap();
    db.create_record(&wrong_person).unwrap();

    let results = db
        .get_timeline(&TimelineQuery {
            limit: 20,
            offset: 0,
            tags: vec!["工作".to_string()],
            persons: vec!["张三".to_string()],
        })
        .unwrap();

    assert_eq!(titles(&results), vec!["标签人物都命中".to_string()]);
}

#[test]
fn test_search_records_matches_chinese_title_substrings() {
    let (db, _temp) = setup_test_db();

    let task = Record::new_task(
        "试试看".to_string(),
        "正文没有额外关键词".to_string(),
        Priority::Medium,
    );
    db.create_record(&task).unwrap();

    for query in ["试", "试试", "试看"] {
        let results = db.search_records(query).unwrap();
        assert_eq!(
            titles(&results),
            vec!["试试看".to_string()],
            "query={query}"
        );
    }
}

#[test]
fn test_search_records_matches_chinese_content_substrings() {
    let (db, _temp) = setup_test_db();

    let task = Record::new_task(
        "搜索修复".to_string(),
        "功能是实现了，但是在设计意图上和我想要的不太一致".to_string(),
        Priority::High,
    );
    db.create_record(&task).unwrap();

    let results = db.search_records("设计意图").unwrap();
    assert_eq!(titles(&results), vec!["搜索修复".to_string()]);

    let results = db.search_records("我想要").unwrap();
    assert_eq!(titles(&results), vec!["搜索修复".to_string()]);
}

#[test]
fn test_search_records_returns_title_only_matches() {
    let (db, _temp) = setup_test_db();

    let task = Record::new_task(
        "标题命中案例".to_string(),
        "正文完全不包含这个片段".to_string(),
        Priority::Low,
    );
    db.create_record(&task).unwrap();

    let results = db.search_records("命中案").unwrap();
    assert_eq!(titles(&results), vec!["标题命中案例".to_string()]);
}

#[test]
fn test_search_records_keeps_and_semantics_for_multiple_terms() {
    let (db, _temp) = setup_test_db();

    let first = Record::new_task(
        "搜索优化".to_string(),
        "把试试看这个标题也搜出来".to_string(),
        Priority::High,
    );
    let second = Record::new_task(
        "搜索优化".to_string(),
        "这里只有搜索，没有另一个关键词".to_string(),
        Priority::Medium,
    );
    db.create_record(&first).unwrap();
    db.create_record(&second).unwrap();

    let results = db.search_records("搜索 试试").unwrap();
    assert_eq!(titles(&results), vec!["搜索优化".to_string()]);
    assert_eq!(results[0].content, "把试试看这个标题也搜出来");
}

#[test]
fn test_search_records_supports_single_character_queries() {
    let (db, _temp) = setup_test_db();

    let task = Record::new_task("单字".to_string(), "搜字".to_string(), Priority::Medium);
    db.create_record(&task).unwrap();

    let results = db.search_records("字").unwrap();
    assert_eq!(titles(&results), vec!["单字".to_string()]);
}

#[test]
fn test_get_records_by_tag_returns_all_matching_types_in_updated_order() {
    let (db, _temp) = setup_test_db();
    let now = Utc::now();

    let mut task = Record::new_task(
        "任务标题".to_string(),
        "任务内容".to_string(),
        Priority::High,
    );
    task.tags = vec!["开发".to_string()];
    task.updated_at = now - Duration::minutes(3);

    let mut note = Record::new_note("记录标题".to_string());
    note.tags = vec!["开发".to_string()];
    note.updated_at = now;

    let mut idea = Record::new_idea("想法标题".to_string());
    idea.tags = vec!["开发".to_string()];
    idea.updated_at = now - Duration::minutes(1);

    db.create_record(&task).unwrap();
    db.create_record(&note).unwrap();
    db.create_record(&idea).unwrap();

    let results = db.get_records_by_tag("开发").unwrap();
    assert_eq!(
        titles(&results),
        vec![
            "记录标题".to_string(),
            "想法标题".to_string(),
            "任务标题".to_string()
        ]
    );
    assert!(results
        .iter()
        .all(|record| record.tags.contains(&"开发".to_string())));
    assert_eq!(results[0].record_type, RecordType::Note);
    assert_eq!(results[1].record_type, RecordType::Idea);
    assert_eq!(results[2].record_type, RecordType::Task);
}

#[test]
fn test_get_records_by_tag_returns_empty_for_unknown_tag() {
    let (db, _temp) = setup_test_db();

    let mut task = Record::new_task(
        "任务标题".to_string(),
        "任务内容".to_string(),
        Priority::Medium,
    );
    task.tags = vec!["开发".to_string()];
    db.create_record(&task).unwrap();

    let results = db.get_records_by_tag("不存在").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_get_records_by_person_returns_all_matching_types_in_updated_order() {
    let (db, _temp) = setup_test_db();
    let now = Utc::now();

    let mut task = Record::new_task(
        "任务标题".to_string(),
        "任务内容".to_string(),
        Priority::High,
    );
    task.persons = vec!["张三".to_string()];
    task.updated_at = now - Duration::minutes(3);

    let mut note = Record::new_note("记录标题".to_string());
    note.persons = vec!["张三".to_string()];
    note.updated_at = now;

    let mut idea = Record::new_idea("想法标题".to_string());
    idea.persons = vec!["张三".to_string()];
    idea.updated_at = now - Duration::minutes(1);

    db.create_record(&task).unwrap();
    db.create_record(&note).unwrap();
    db.create_record(&idea).unwrap();

    let results = db.get_records_by_person("张三").unwrap();
    assert_eq!(
        titles(&results),
        vec![
            "记录标题".to_string(),
            "想法标题".to_string(),
            "任务标题".to_string()
        ]
    );
    assert!(results
        .iter()
        .all(|record| record.persons.contains(&"张三".to_string())));
    assert_eq!(results[0].record_type, RecordType::Note);
    assert_eq!(results[1].record_type, RecordType::Idea);
    assert_eq!(results[2].record_type, RecordType::Task);
}

#[test]
fn test_get_records_by_person_returns_empty_for_unknown_person() {
    let (db, _temp) = setup_test_db();

    let mut task = Record::new_task(
        "任务标题".to_string(),
        "任务内容".to_string(),
        Priority::Medium,
    );
    task.persons = vec!["张三".to_string()];
    db.create_record(&task).unwrap();

    let results = db.get_records_by_person("不存在").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_get_all_records_returns_all_types_in_updated_order() {
    let (db, _temp) = setup_test_db();
    let now = Utc::now();

    let mut task = Record::new_task(
        "任务标题".to_string(),
        "任务内容".to_string(),
        Priority::High,
    );
    task.updated_at = now - Duration::minutes(2);

    let mut note = Record::new_note("记录标题".to_string());
    note.updated_at = now;

    let mut idea = Record::new_idea("想法标题".to_string());
    idea.updated_at = now - Duration::minutes(1);

    db.create_record(&task).unwrap();
    db.create_record(&note).unwrap();
    db.create_record(&idea).unwrap();

    let results = db.get_all_records().unwrap();
    assert_eq!(
        titles(&results),
        vec![
            "记录标题".to_string(),
            "想法标题".to_string(),
            "任务标题".to_string()
        ]
    );
    assert_eq!(results[0].record_type, RecordType::Note);
    assert_eq!(results[1].record_type, RecordType::Idea);
    assert_eq!(results[2].record_type, RecordType::Task);
}

// ============================================================================
// 任务生命周期验证
// ============================================================================

/// 验证：update_record_notified_at 不会覆盖用户的并发修改（如标题变更）
#[test]
fn test_update_notified_at_does_not_overwrite_other_fields() {
    let (db, _temp) = setup_test_db();

    let mut task = Record::new_task(
        "原始标题".to_string(),
        "内容".to_string(),
        Priority::Medium,
    );
    db.create_record(&task).unwrap();

    // 模拟用户并发修改标题（实际场景中这是另一个线程/进程的操作）
    task.title = Some("用户修改后的标题".to_string());
    db.create_record(&task).unwrap();

    // 模拟提醒线程只更新 notified_at
    let now = Utc::now();
    db.update_record_notified_at(task.id, now).unwrap();

    // 重新读取，验证标题仍然是用户修改后的值，没有被旧数据覆盖
    let tasks = db.get_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, Some("用户修改后的标题".to_string()));
    assert!(tasks[0].notified_at.is_some());
}
