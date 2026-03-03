use beitang::models::Priority;

// 复制 parse_input_static 逻辑用于测试
fn parse_input(input: &str) -> (String, Priority) {
    let trimmed = input.trim();
    if trimmed.starts_with("!!") {
        let content = trimmed[2..].trim_start();
        (content.to_string(), Priority::High)
    } else if trimmed.starts_with("!") {
        let content = trimmed[1..].trim_start();
        (content.to_string(), Priority::Medium)
    } else {
        (trimmed.to_string(), Priority::Low)
    }
}

#[test]
fn test_parse_input_high_priority_with_space() {
    let result = parse_input("!! High priority task");
    assert_eq!(result.0, "High priority task");
    assert_eq!(result.1, Priority::High);
}

#[test]
fn test_parse_input_high_priority_without_space() {
    let result = parse_input("!!High priority task");
    assert_eq!(result.0, "High priority task");
    assert_eq!(result.1, Priority::High);
}

#[test]
fn test_parse_input_medium_priority_with_space() {
    let result = parse_input("! Medium priority task");
    assert_eq!(result.0, "Medium priority task");
    assert_eq!(result.1, Priority::Medium);
}

#[test]
fn test_parse_input_medium_priority_without_space() {
    let result = parse_input("!Medium priority task");
    assert_eq!(result.0, "Medium priority task");
    assert_eq!(result.1, Priority::Medium);
}

#[test]
fn test_parse_input_low_priority() {
    let result = parse_input("Just a normal task");
    assert_eq!(result.0, "Just a normal task");
    assert_eq!(result.1, Priority::Low);
}

#[test]
fn test_parse_input_trims_whitespace() {
    let result = parse_input("  !!  Task with extra spaces  ");
    assert_eq!(result.0, "Task with extra spaces");
    assert_eq!(result.1, Priority::High);
}

#[test]
fn test_parse_input_empty_after_priority() {
    let result = parse_input("!!");
    assert_eq!(result.0, "");
    assert_eq!(result.1, Priority::High);
}
