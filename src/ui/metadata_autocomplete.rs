use std::ops::Range;

use gpui::{
    div, px, rgb, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Styled as _,
};
use gpui_component::input::{InputState, Rope, RopeExt};
use gpui_component::scroll::ScrollableElement;

use crate::models::MetadataCatalogEntry;

const MAX_VISIBLE_MATCHES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKind {
    Tag,
    Person,
}

impl MetadataKind {
    pub fn marker(self) -> char {
        match self {
            Self::Tag => '#',
            Self::Person => '@',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveMetadataQuery {
    pub kind: MetadataKind,
    pub range: Range<usize>,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataCompletionEdit {
    pub text: String,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataCatalog {
    pub tags: Vec<MetadataCatalogEntry>,
    pub persons: Vec<MetadataCatalogEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct MetadataAutocompleteState {
    catalog: MetadataCatalog,
    active_query: Option<ActiveMetadataQuery>,
    matches: Vec<MetadataCatalogEntry>,
    selected_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataAutocompleteAction {
    Ignored,
    Moved,
    Dismissed,
    Applied(MetadataCompletionEdit),
}

impl MetadataAutocompleteState {
    pub fn set_catalog(&mut self, catalog: MetadataCatalog) {
        self.catalog = catalog;
    }

    pub fn sync_from_input(&mut self, input: &InputState) {
        self.sync(input.text().to_string().as_str(), input.cursor());
    }

    pub fn sync(&mut self, text: &str, cursor: usize) {
        let Some(query) = detect_active_metadata_query(text, cursor) else {
            self.clear();
            return;
        };

        let matches = filter_metadata_candidates(self.catalog_for(query.kind), &query.query);
        if matches.is_empty() {
            self.clear();
            return;
        }

        let next_selected = self
            .selected_entry()
            .and_then(|selected| matches.iter().position(|entry| entry.name == selected.name))
            .unwrap_or(0);

        self.active_query = Some(query);
        self.matches = matches;
        self.selected_index = next_selected.min(self.matches.len().saturating_sub(1));
    }

    pub fn clear(&mut self) {
        self.active_query = None;
        self.matches.clear();
        self.selected_index = 0;
    }

    pub fn is_open(&self) -> bool {
        self.active_query.is_some() && !self.matches.is_empty()
    }

    pub fn visible_match_count(&self) -> usize {
        self.matches.len().min(MAX_VISIBLE_MATCHES)
    }

    pub fn menu_height(&self) -> Pixels {
        if !self.is_open() {
            return px(0.0);
        }

        px(self.visible_match_count() as f32 * 32.0 + 12.0)
    }

    pub fn matches(&self) -> &[MetadataCatalogEntry] {
        &self.matches
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn selected_entry(&self) -> Option<&MetadataCatalogEntry> {
        self.matches.get(self.selected_index)
    }

    pub fn handle_key(&mut self, key: &str, text: &str) -> MetadataAutocompleteAction {
        if !self.is_open() {
            return MetadataAutocompleteAction::Ignored;
        }

        match key {
            "up" => {
                if !self.matches.is_empty() {
                    self.selected_index =
                        (self.selected_index + self.matches.len() - 1) % self.matches.len();
                }
                MetadataAutocompleteAction::Moved
            }
            "down" => {
                if !self.matches.is_empty() {
                    self.selected_index = (self.selected_index + 1) % self.matches.len();
                }
                MetadataAutocompleteAction::Moved
            }
            "tab" => self
                .apply_selected(text)
                .map(MetadataAutocompleteAction::Applied)
                .unwrap_or(MetadataAutocompleteAction::Ignored),
            "escape" => {
                self.clear();
                MetadataAutocompleteAction::Dismissed
            }
            _ => MetadataAutocompleteAction::Ignored,
        }
    }

    pub fn apply_selected(&mut self, text: &str) -> Option<MetadataCompletionEdit> {
        let selected_name = self.selected_entry()?.name.clone();
        let edit = self.apply_candidate(text, &selected_name)?;
        self.clear();
        Some(edit)
    }

    pub fn apply_index(&mut self, text: &str, index: usize) -> Option<MetadataCompletionEdit> {
        let name = self.matches.get(index)?.name.clone();
        let edit = self.apply_candidate(text, &name)?;
        self.clear();
        Some(edit)
    }

    pub fn upsert_entries(&mut self, tags: &[String], persons: &[String]) {
        for name in tags {
            upsert_entry(&mut self.catalog.tags, name);
        }
        for name in persons {
            upsert_entry(&mut self.catalog.persons, name);
        }
    }

    fn apply_candidate(&self, text: &str, name: &str) -> Option<MetadataCompletionEdit> {
        let query = self.active_query.as_ref()?;
        Some(apply_metadata_completion(
            text,
            query.range.clone(),
            query.kind,
            name,
        ))
    }

    fn catalog_for(&self, kind: MetadataKind) -> &[MetadataCatalogEntry] {
        match kind {
            MetadataKind::Tag => &self.catalog.tags,
            MetadataKind::Person => &self.catalog.persons,
        }
    }
}

fn upsert_entry(entries: &mut Vec<MetadataCatalogEntry>, name: &str) {
    if let Some(existing) = entries.iter_mut().find(|entry| entry.name == name) {
        existing.usage_count += 1;
    } else {
        entries.push(MetadataCatalogEntry {
            name: name.to_string(),
            usage_count: 1,
        });
    }

    entries.sort_by(|left, right| {
        right
            .usage_count
            .cmp(&left.usage_count)
            .then_with(|| left.name.cmp(&right.name))
    });
}

pub fn detect_active_metadata_query(text: &str, cursor: usize) -> Option<ActiveMetadataQuery> {
    if text.is_empty() {
        return None;
    }

    let cursor = clip_to_char_boundary(text, cursor);
    let prefix = &text[..cursor];
    let mut marker_start = None;
    for (idx, ch) in prefix.char_indices().rev() {
        if ch.is_whitespace() {
            break;
        }
        if ch == '#' || ch == '@' {
            marker_start = Some(idx);
            break;
        }
    }
    let marker_start = marker_start?;

    if marker_start > 0
        && text[..marker_start]
            .chars()
            .next_back()
            .is_some_and(disallow_inline_marker_after)
    {
        return None;
    }

    let marker = text[marker_start..].chars().next()?;
    let kind = match marker {
        '#' => MetadataKind::Tag,
        '@' => MetadataKind::Person,
        _ => return None,
    };

    let token_start = marker_start + marker.len_utf8();
    let token_end = find_token_end(text, token_start);
    if cursor < token_start || cursor > token_end {
        return None;
    }

    let query = &text[token_start..cursor];
    if query
        .chars()
        .any(|ch| ch.is_whitespace() || ch == '#' || ch == '@')
    {
        return None;
    }

    Some(ActiveMetadataQuery {
        kind,
        range: marker_start..token_end,
        query: query.to_string(),
    })
}

pub fn filter_metadata_candidates(
    candidates: &[MetadataCatalogEntry],
    query: &str,
) -> Vec<MetadataCatalogEntry> {
    if query.is_empty() {
        return candidates
            .iter()
            .take(MAX_VISIBLE_MATCHES)
            .cloned()
            .collect();
    }

    let query_folded = query.to_lowercase();
    let mut matched = candidates
        .iter()
        .filter_map(
            |entry| match match_kind(entry.name.as_str(), &query_folded) {
                Some(kind) => Some((kind, entry.clone())),
                None => None,
            },
        )
        .collect::<Vec<_>>();

    matched.sort_by(|(left_kind, left), (right_kind, right)| {
        left_kind
            .cmp(right_kind)
            .then_with(|| right.usage_count.cmp(&left.usage_count))
            .then_with(|| left.name.cmp(&right.name))
    });

    matched
        .into_iter()
        .take(MAX_VISIBLE_MATCHES)
        .map(|(_, entry)| entry)
        .collect()
}

pub fn apply_metadata_completion(
    text: &str,
    range: Range<usize>,
    kind: MetadataKind,
    name: &str,
) -> MetadataCompletionEdit {
    let needs_leading_space = text[..range.start]
        .chars()
        .next_back()
        .is_some_and(|ch| !ch.is_whitespace());
    let needs_trailing_space = range.end == text.len();

    let mut result = String::with_capacity(
        text.len()
            + name.len()
            + kind.marker().len_utf8()
            + usize::from(needs_leading_space)
            + usize::from(needs_trailing_space),
    );
    result.push_str(&text[..range.start]);
    if needs_leading_space {
        result.push(' ');
    }
    result.push(kind.marker());
    result.push_str(name);
    if needs_trailing_space {
        result.push(' ');
    }
    result.push_str(&text[range.end..]);

    let cursor = range.start
        + usize::from(needs_leading_space)
        + kind.marker().len_utf8()
        + name.len()
        + usize::from(needs_trailing_space);

    MetadataCompletionEdit {
        text: result,
        cursor,
    }
}

pub fn apply_completion_to_input<T>(
    input: &gpui::Entity<InputState>,
    edit: &MetadataCompletionEdit,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<T>,
) {
    let next_text = edit.text.clone();
    let next_cursor = edit.cursor;
    input.update(cx, |state, cx| {
        state.set_value(&next_text, window, cx);
        let position = Rope::from(next_text.as_str()).offset_to_position(next_cursor);
        state.set_cursor_position(position, window, cx);
        state.focus(window, cx);
    });
}

pub fn render_autocomplete_menu(
    state: &MetadataAutocompleteState,
    id_prefix: &str,
    _cx: &App,
    render_item: impl Fn(usize, &MetadataCatalogEntry, bool) -> gpui::AnyElement,
) -> gpui::AnyElement {
    div()
        .id(format!("{id_prefix}-menu"))
        .w_full()
        .mt(px(6.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(0xe5e7eb))
        .bg(rgb(0xffffff))
        .overflow_hidden()
        .child(
            div()
                .max_h(state.menu_height())
                .overflow_y_scrollbar()
                .child(
                    div()
                        .py(px(6.0))
                        .children(state.matches().iter().enumerate().map(|(idx, candidate)| {
                            render_item(idx, candidate, idx == state.selected_index())
                        })),
                ),
        )
        .into_any_element()
}

pub fn autocomplete_item(
    id: impl Into<gpui::ElementId>,
    label: &str,
    usage_count: usize,
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .px(px(10.0))
        .py(px(7.0))
        .rounded(px(8.0))
        .mx(px(6.0))
        .bg(if selected {
            rgb(0xe6f4ff)
        } else {
            rgb(0xffffff)
        })
        .hover(|style| style.bg(rgb(0xf5f5f5)))
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x262626))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x999999))
                        .child(format!("{} 次", usage_count)),
                ),
        )
}

fn find_token_end(text: &str, start: usize) -> usize {
    for (rel_idx, ch) in text[start..].char_indices() {
        if ch.is_whitespace() || ch == '#' || ch == '@' {
            return start + rel_idx;
        }
    }

    text.len()
}

fn clip_to_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn disallow_inline_marker_after(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '#' || ch == '@'
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchKind {
    Prefix,
    Contains,
    Subsequence,
}

fn match_kind(candidate: &str, folded_query: &str) -> Option<MatchKind> {
    let folded_candidate = candidate.to_lowercase();
    if folded_candidate.starts_with(folded_query) {
        return Some(MatchKind::Prefix);
    }
    if folded_candidate.contains(folded_query) {
        return Some(MatchKind::Contains);
    }
    if is_subsequence(folded_query, &folded_candidate) {
        return Some(MatchKind::Subsequence);
    }
    None
}

fn is_subsequence(query: &str, candidate: &str) -> bool {
    let mut query_chars = query.chars();
    let mut current = query_chars.next();

    for ch in candidate.chars() {
        if Some(ch) == current {
            current = query_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }

    current.is_none()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_metadata_completion, detect_active_metadata_query, filter_metadata_candidates,
        ActiveMetadataQuery, MetadataAutocompleteState, MetadataCatalog, MetadataCatalogEntry,
        MetadataKind,
    };

    #[test]
    fn detect_query_for_empty_marker() {
        let query = detect_active_metadata_query("计划 #", "计划 #".len()).unwrap();
        assert_eq!(
            query,
            ActiveMetadataQuery {
                kind: MetadataKind::Tag,
                range: 7..8,
                query: String::new(),
            }
        );
    }

    #[test]
    fn detect_query_inside_token() {
        let text = "联系 @zhangsan 今天同步";
        let cursor = text.find("ang").unwrap();
        let query = detect_active_metadata_query(text, cursor).unwrap();
        assert_eq!(query.kind, MetadataKind::Person);
        assert_eq!(query.query, "zh");
    }

    #[test]
    fn detect_query_ignores_email_like_marker() {
        let text = "alice@bob";
        assert!(detect_active_metadata_query(text, text.len()).is_none());
    }

    #[test]
    fn detect_query_allows_marker_after_cjk_text() {
        let text = "今天联系@";
        let query = detect_active_metadata_query(text, text.len()).unwrap();
        assert_eq!(query.kind, MetadataKind::Person);
        assert_eq!(query.query, "");
    }

    #[test]
    fn apply_completion_handles_multibyte_cursor() {
        let edit = apply_metadata_completion(
            "今天跟进 #开",
            "今天跟进 ".len().."今天跟进 #开".len(),
            MetadataKind::Tag,
            "开发",
        );
        assert_eq!(edit.text, "今天跟进 #开发 ");
        assert_eq!(edit.cursor, "今天跟进 #开发 ".len());
    }

    #[test]
    fn apply_completion_inserts_leading_space_when_needed() {
        let edit = apply_metadata_completion(
            "今天联系@a",
            "今天联系".len().."今天联系@a".len(),
            MetadataKind::Person,
            "张三",
        );
        assert_eq!(edit.text, "今天联系 @张三 ");
        assert_eq!(edit.cursor, "今天联系 @张三 ".len());
    }

    #[test]
    fn filter_candidates_prefers_prefix_then_usage() {
        let entries = vec![
            MetadataCatalogEntry {
                name: "开发".to_string(),
                usage_count: 5,
            },
            MetadataCatalogEntry {
                name: "协作开发".to_string(),
                usage_count: 20,
            },
            MetadataCatalogEntry {
                name: "开发测试".to_string(),
                usage_count: 8,
            },
        ];

        let filtered = filter_metadata_candidates(&entries, "开发");
        assert_eq!(filtered[0].name, "开发测试");
        assert_eq!(filtered[1].name, "开发");
        assert_eq!(filtered[2].name, "协作开发");
    }

    #[test]
    fn autocomplete_state_applies_selected_candidate() {
        let mut state = MetadataAutocompleteState::default();
        state.set_catalog(MetadataCatalog {
            tags: vec![MetadataCatalogEntry {
                name: "开发".to_string(),
                usage_count: 3,
            }],
            persons: Vec::new(),
        });
        state.sync("今天 #开", "今天 #开".len());

        let edit = state.apply_selected("今天 #开").unwrap();
        assert_eq!(edit.text, "今天 #开发 ");
    }
}
