use robinne::ui::style::TaskTypography;

#[test]
fn task_typography_uses_small_scale_and_light_weights() {
    assert_eq!(TaskTypography::SYSTEM_FONT_FAMILY, ".SystemUIFont");
    assert_eq!(TaskTypography::PAGE_TITLE_SIZE, 23.0);
    assert_eq!(TaskTypography::TASK_TITLE_SIZE, 14.0);
    assert_eq!(TaskTypography::META_SIZE, 12.5);
    assert_eq!(TaskTypography::SIDEBAR_ITEM_SIZE, 14.0);
    assert!(TaskTypography::TASK_TITLE_WEIGHT <= 500);
    assert!(TaskTypography::DETAIL_TITLE_WEIGHT <= 520);
    assert!(TaskTypography::MAX_DEFAULT_WEIGHT < 600);
}
