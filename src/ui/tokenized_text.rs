use gpui::*;
use gpui_component::{h_flex, v_flex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextTokenSegment {
    Plain(String),
    Tag(String),
    Person(String),
}

#[derive(Clone, Copy)]
pub enum MetadataChipKind {
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
        MetadataChipKind::Tag => label.trim_start_matches('#'),
        MetadataChipKind::Person => label.trim_start_matches('@'),
    };
    let (background, border, text_color) = match kind {
        MetadataChipKind::Tag => (rgb(0xf7f3ee), rgb(0xe5ddd2), rgb(0x6a5847)),
        MetadataChipKind::Person => (rgb(0xecf6ff), rgb(0xcfe7fb), rgb(0x1677ff)),
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
        TextTokenSegment::Tag(text) => {
            render_emphasized_token_segment(text, rgb(0x1677ff), FontWeight::MEDIUM)
        }
        TextTokenSegment::Person(text) => {
            render_emphasized_token_segment(text, rgb(0x8c6a45), FontWeight::MEDIUM)
        }
    }
}

fn tokenize_line(line: &str) -> Vec<TextTokenSegment> {
    if line.is_empty() {
        return vec![TextTokenSegment::Plain(String::new())];
    }

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_whitespace = None;

    for ch in line.chars() {
        let is_whitespace = ch.is_whitespace();
        match in_whitespace {
            Some(state) if state == is_whitespace => current.push(ch),
            Some(_) => {
                push_segment(&mut segments, &current);
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
        push_segment(&mut segments, &current);
    }

    segments
}

fn push_segment(segments: &mut Vec<TextTokenSegment>, value: &str) {
    if value.is_empty() {
        return;
    }

    if value.chars().all(char::is_whitespace) {
        segments.push(TextTokenSegment::Plain(value.to_string()));
        return;
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

fn render_emphasized_token_segment(text: &str, color: Rgba, weight: FontWeight) -> AnyElement {
    div()
        .text_color(color)
        .font_weight(weight)
        .child(text.to_string())
        .into_any_element()
}
