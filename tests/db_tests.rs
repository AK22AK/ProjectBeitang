use beitang::db::Database;
use beitang::models::{Priority, Record, RecordType};
use tempfile::TempDir;

fn setup_test_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    (db, temp_dir)
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

    let tasks = db.get_tasks(false).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, Some("Test Title".to_string()));
    assert_eq!(tasks[0].content, "Test content");
    assert_eq!(tasks[0].priority, Some(Priority::High));
    assert_eq!(tasks[0].record_type, RecordType::Task);
}

#[test]
fn test_get_tasks_returns_empty_when_no_tasks() {
    let (db, _temp) = setup_test_db();

    let tasks = db.get_tasks(false).unwrap();
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

    let tasks = db.get_tasks(false).unwrap();
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

    let tasks = db.get_tasks(false).unwrap();

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

    let tasks = db.get_tasks(false).unwrap();
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
    let tasks = db.get_tasks(false).unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].is_completed());
    assert!(tasks[0].completed_at.is_some());
}
