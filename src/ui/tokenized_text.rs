use gpui::*;
use gpui_component::{h_flex, v_flex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextTokenSegment {
    Plain(String),
    Line(String),
    Tag(String),
    Person(String),
}

#[derive(Clone, Copy)]
pub enum MetadataChipKind {
    Line,
    Tag,
    Person,
}

#[derive(Clone, Copy)]
pub struct TokenTextStyle {
    pub color: Rgba,
    pub weight: FontWeight,
}

impl TokenTextStyle {
    pub fn new(color: Rgba, weight: FontWeight) -> Self {
        Self { color, weight }
    }
}

pub fn tokenize_text(text: &str) -> Vec<Vec<TextTokenSegment>> {
    let mut lines = text
        .split('\n')
        .map(tokenize_line)
        .collect::<Vec<Vec<TextTokenSegment>>>();

    if lines.is_empty() {
        lines.push(vec![TextTokenSegment::Plain(String::new())]);
    }

    lines
}

pub fn render_tokenized_text(text: &str, style: TokenTextStyle) -> AnyElement {
    let lines = tokenize_text(text);
    if lines.len() == 1 {
        return render_token_line(&lines[0], style).into_any_element();
    }

    v_flex()
        .w_full()
        .gap(px(4.0))
        .children(lines.iter().map(|line| render_token_line(line, style)))
        .into_any_element()
}

pub fn render_metadata_chip(kind: MetadataChipKind, label: &str) -> AnyElement {
    let label = match kind {
        MetadataChipKind::Line => label.trim_start_matches('~').trim_start_matches('～'),
        MetadataChipKind::Tag => label.trim_start_matches('#'),
        MetadataChipKind::Person => label.trim_start_matches('@'),
    };
    let (background, border, text_color) = match kind {
        MetadataChipKind::Line => (rgb(0xecfdf5), rgb(0xbbf7d0), rgb(0x047857)),
        MetadataChipKind::Tag => (rgb(0xeff6ff), rgb(0xbfdbfe), rgb(0x1d4ed8)),
        MetadataChipKind::Person => (rgb(0xfff7ed), rgb(0xfed7aa), rgb(0xc2410c)),
    };

    div()
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(999.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(text_color)
        .child(label.to_string())
        .into_any_element()
}

pub fn render_inline_token_text(token: &TextTokenSegment, style: TokenTextStyle) -> AnyElement {
    match token {
        TextTokenSegment::Plain(text) => render_plain_text_segment(text, style),
        TextTokenSegment::Line(text) => render_emphasized_token_segment(
            text,
            rgb(0x047857),
            rgb(0xecfdf5),
            FontWeight::SEMIBOLD,
        ),
        TextTokenSegment::Tag(text) => render_emphasized_token_segment(
            text,
            rgb(0x1d4ed8),
            rgb(0xeff6ff),
            FontWeight::SEMIBOLD,
        ),
        TextTokenSegment::Person(text) => render_emphasized_token_segment(
            text,
            rgb(0xc2410c),
            rgb(0xfff7ed),
            FontWeight::SEMIBOLD,
        ),
    }
}

fn tokenize_line(line: &str) -> Vec<TextTokenSegment> {
    if line.is_empty() {
        return vec![TextTokenSegment::Plain(String::new())];
    }

    let mut segments = Vec::new();
    let mut plain = String::new();
    let mut chars = line.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if !is_metadata_marker(ch) {
            plain.push(ch);
            continue;
        }

        let mut token = String::new();
        token.push(ch);
        while let Some(&(_, next)) = chars.peek() {
            if is_metadata_delimiter(next) {
                break;
            }
            token.push(next);
            chars.next();
        }

        if token.chars().count() > 1 {
            flush_plain_segments(&mut segments, &mut plain);
            push_segment(&mut segments, &token);
        } else {
            plain.push(ch);
        }
    }

    flush_plain_segments(&mut segments, &mut plain);

    segments
}

fn flush_plain_segments(segments: &mut Vec<TextTokenSegment>, plain: &mut String) {
    if plain.is_empty() {
        return;
    }

    let mut current = String::new();
    let mut in_whitespace = None;
    for ch in plain.chars() {
        let is_whitespace = ch.is_whitespace();
        match in_whitespace {
            Some(state) if state == is_whitespace => current.push(ch),
            Some(_) => {
                push_segment(segments, &current);
                current.clear();
                current.push(ch);
                in_whitespace = Some(is_whitespace);
            }
            None => {
                current.push(ch);
                in_whitespace = Some(is_whitespace);
            }
        }
    }

    if !current.is_empty() {
        push_segment(segments, &current);
    }
    plain.clear();
}

fn is_metadata_marker(ch: char) -> bool {
    matches!(ch, '#' | '@' | '~' | '～')
}

fn is_metadata_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || is_metadata_marker(ch)
        || matches!(
            ch,
            ',' | '，'
                | '.'
                | '。'
                | '!'
                | '！'
                | '?'
                | '？'
                | ';'
                | '；'
                | ':'
                | '：'
                | '、'
                | '('
                | ')'
                | '（'
                | '）'
                | '['
                | ']'
                | '【'
                | '】'
                | '{'
                | '}'
                | '<'
                | '>'
                | '《'
                | '》'
                | '"'
                | '\''
                | '“'
                | '”'
                | '‘'
                | '’'
                | '`'
        )
}

fn push_segment(segments: &mut Vec<TextTokenSegment>, value: &str) {
    if value.is_empty() {
        return;
    }

    if value.chars().all(char::is_whitespace) {
        segments.push(TextTokenSegment::Plain(value.to_string()));
        return;
    }

    if value.starts_with('~') || value.starts_with('～') {
        if value.chars().count() > 1 {
            segments.push(TextTokenSegment::Line(value.to_string()));
            return;
        }
    }

    if let Some(tag) = value.strip_prefix('#') {
        if !tag.is_empty() {
            segments.push(TextTokenSegment::Tag(value.to_string()));
            return;
        }
    }

    if let Some(person) = value.strip_prefix('@') {
        if !person.is_empty() {
            segments.push(TextTokenSegment::Person(value.to_string()));
            return;
        }
    }

    segments.push(TextTokenSegment::Plain(value.to_string()));
}

fn render_token_line(tokens: &[TextTokenSegment], style: TokenTextStyle) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_w(px(0.0))
        .gap(px(0.0))
        .flex_wrap()
        .items_center()
        .children(
            tokens
                .iter()
                .map(|token| render_inline_token_text(token, style)),
        )
}

fn render_plain_text_segment(text: &str, style: TokenTextStyle) -> AnyElement {
    div()
        .text_color(style.color)
        .font_weight(style.weight)
        .child(if text.is_empty() {
            "\u{00a0}".to_string()
        } else {
            text.to_string()
        })
        .into_any_element()
}

fn render_emphasized_token_segment(
    text: &str,
    color: Rgba,
    background: Rgba,
    weight: FontWeight,
) -> AnyElement {
    div()
        .px(px(3.0))
        .rounded(px(4.0))
        .bg(background)
        .text_color(color)
        .font_weight(weight)
        .child(text.to_string())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{tokenize_text, TextTokenSegment};

    #[test]
    fn tokenizes_line_tag_and_person_with_distinct_segments() {
        let tokens = tokenize_text("推进 ~Robinne/事务 #测试 @self");
        assert_eq!(
            tokens[0],
            vec![
                TextTokenSegment::Plain("推进".to_string()),
                TextTokenSegment::Plain(" ".to_string()),
                TextTokenSegment::Line("~Robinne/事务".to_string()),
                TextTokenSegment::Plain(" ".to_string()),
                TextTokenSegment::Tag("#测试".to_string()),
                TextTokenSegment::Plain(" ".to_string()),
                TextTokenSegment::Person("@self".to_string()),
            ]
        );
    }

    #[test]
    fn tokenizes_fullwidth_line_marker() {
        let tokens = tokenize_text("～微信冻屏问题关键进展 #关键进展");
        assert_eq!(
            tokens[0][0],
            TextTokenSegment::Line("～微信冻屏问题关键进展".to_string())
        );
    }

    #[test]
    fn tokenizes_person_marker_adjacent_to_chinese_text() {
        let tokens = tokenize_text("发给了@沈慧海 @宋亚南");
        assert_eq!(
            tokens[0],
            vec![
                TextTokenSegment::Plain("发给了".to_string()),
                TextTokenSegment::Person("@沈慧海".to_string()),
                TextTokenSegment::Plain(" ".to_string()),
                TextTokenSegment::Person("@宋亚南".to_string()),
            ]
        );
    }
}
