use crate::models::{Record, RecordType, TaskStatus};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 60;
const TARGET_CHUNK_CHARS: usize = 6_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AiProviderProtocol {
    #[default]
    OpenAiCompatible,
    Anthropic,
}

impl AiProviderProtocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI 格式",
            Self::Anthropic => "Anthropic 格式",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => DEFAULT_OPENAI_BASE_URL,
            Self::Anthropic => DEFAULT_ANTHROPIC_BASE_URL,
        }
    }

    pub fn api_key_env_var(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiSettings {
    #[serde(default)]
    pub protocol: AiProviderProtocol,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            protocol: AiProviderProtocol::OpenAiCompatible,
            base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            model: String::new(),
            request_timeout_secs: default_request_timeout_secs(),
        }
    }
}

impl AiSettings {
    pub fn normalized_base_url(&self) -> String {
        let trimmed = self.base_url.trim();
        if trimmed.is_empty() {
            self.protocol.default_base_url().to_string()
        } else {
            trimmed.trim_end_matches('/').to_string()
        }
    }

    pub fn has_connection_config(&self) -> bool {
        !self.normalized_base_url().is_empty() && !self.model.trim().is_empty()
    }
}

fn default_request_timeout_secs() -> u64 {
    DEFAULT_REQUEST_TIMEOUT_SECS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSummaryMode {
    PastSummary,
    FutureTasks,
}

impl AiSummaryMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::PastSummary => "过去总结",
            Self::FutureTasks => "未来提炼",
        }
    }

    fn prompt_goal(self) -> &'static str {
        match self {
            Self::PastSummary => "提炼这段时间已经完成、推进和暴露出来的工作事实",
            Self::FutureTasks => "基于这段时间的事实和仍未完成的事项，提炼接下来最值得推进的工作",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AiContextQuery {
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub persons: Vec<String>,
    pub mode: AiSummaryMode,
}

#[derive(Debug, Clone)]
pub struct AiContextBundle {
    pub records: Vec<Record>,
    pub day_blocks: Vec<AiDayBlock>,
    pub chunk_count: usize,
    pub estimated_chars: usize,
    pub task_count: usize,
    pub open_task_count: usize,
    pub completed_task_count: usize,
    pub note_like_count: usize,
    pub date_span_label: String,
}

#[derive(Debug, Clone)]
pub struct AiDayBlock {
    pub label: String,
    pub rendered: String,
}

pub fn local_day_range_to_utc(
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    if end_date < start_date {
        return Err("结束日期不能早于开始日期".to_string());
    }

    let Some(start_naive) = start_date.and_hms_opt(0, 0, 0) else {
        return Err("开始日期无效".to_string());
    };
    let Some(end_naive) = end_date.and_hms_opt(23, 59, 59) else {
        return Err("结束日期无效".to_string());
    };

    let Some(start_local) = Local.from_local_datetime(&start_naive).single() else {
        return Err("开始日期无法转换为本地时间".to_string());
    };
    let Some(end_local) = Local.from_local_datetime(&end_naive).single() else {
        return Err("结束日期无法转换为本地时间".to_string());
    };

    Ok((
        start_local.with_timezone(&Utc),
        end_local.with_timezone(&Utc),
    ))
}

pub fn build_context_bundle(records: &[Record], query: &AiContextQuery) -> AiContextBundle {
    let mut filtered = records
        .iter()
        .filter(|record| record_matches_query(record, query))
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by_key(primary_sort_timestamp);

    let mut task_count = 0usize;
    let mut open_task_count = 0usize;
    let mut completed_task_count = 0usize;
    let mut note_like_count = 0usize;
    let mut grouped: BTreeMap<NaiveDate, Vec<Record>> = BTreeMap::new();

    for record in &filtered {
        match record.record_type {
            RecordType::Task => {
                task_count += 1;
                if matches!(
                    record.status,
                    Some(TaskStatus::Todo) | Some(TaskStatus::InProgress)
                ) && record.completed_at.is_none()
                {
                    open_task_count += 1;
                }
                if record.completed_at.is_some() || record.status == Some(TaskStatus::Done) {
                    completed_task_count += 1;
                }
            }
            RecordType::Note | RecordType::Idea | RecordType::Event => {
                note_like_count += 1;
            }
        }

        let local_date = primary_sort_timestamp(record)
            .with_timezone(&Local)
            .date_naive();
        grouped.entry(local_date).or_default().push(record.clone());
    }

    let day_blocks = grouped
        .into_iter()
        .map(|(date, items)| AiDayBlock {
            label: date.format("%Y-%m-%d").to_string(),
            rendered: render_day_block(date, &items),
        })
        .collect::<Vec<_>>();

    let estimated_chars = day_blocks
        .iter()
        .map(|block| block.rendered.len())
        .sum::<usize>();
    let chunk_count = split_day_blocks(&day_blocks).len().max(1);

    AiContextBundle {
        records: filtered,
        day_blocks,
        chunk_count,
        estimated_chars,
        task_count,
        open_task_count,
        completed_task_count,
        note_like_count,
        date_span_label: format!(
            "{} 至 {}",
            query.start_at.with_timezone(&Local).format("%Y-%m-%d"),
            query.end_at.with_timezone(&Local).format("%Y-%m-%d")
        ),
    }
}

pub fn generate_summary(
    settings: &AiSettings,
    api_key: &str,
    query: &AiContextQuery,
    bundle: &AiContextBundle,
) -> Result<String, String> {
    if !settings.has_connection_config() {
        return Err("AI 连接未配置完整，请先填写 Base URL 和 Model".to_string());
    }
    if api_key.trim().is_empty() {
        return Err("当前协议未配置 API Key".to_string());
    }
    if bundle.records.is_empty() {
        return Err("当前筛选范围内没有可用于总结的数据".to_string());
    }

    let client = AiClient::new(settings, api_key)?;
    let chunks = split_day_blocks(&bundle.day_blocks);
    if chunks.len() <= 1 {
        let user_prompt = build_final_prompt(query, bundle, &render_chunks(&chunks));
        return client.request(summary_system_prompt(query.mode), &user_prompt, 1_600);
    }

    let mut partials = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        let prompt = build_chunk_prompt(query, bundle, idx + 1, chunks.len(), chunk);
        let partial = client.request(chunk_system_prompt(query.mode), &prompt, 1_100)?;
        partials.push(partial);
    }

    let merge_prompt = build_merge_prompt(query, bundle, &partials);
    client.request(summary_system_prompt(query.mode), &merge_prompt, 1_600)
}

pub fn test_connection(settings: &AiSettings, api_key: &str) -> Result<String, String> {
    if !settings.has_connection_config() {
        return Err("AI 连接未配置完整，请先填写 Base URL 和 Model".to_string());
    }
    if api_key.trim().is_empty() {
        return Err("当前协议未配置 API Key".to_string());
    }

    let client = AiClient::new(settings, api_key)?;
    let _ = client.request("你是连接测试助手。收到请求后只回复 OK。", "只回复 OK。", 16)?;

    Ok(format!(
        "测试连接成功：{} / {}",
        settings.protocol.label(),
        settings.model.trim()
    ))
}

fn record_matches_query(record: &Record, query: &AiContextQuery) -> bool {
    if !query.tags.is_empty()
        && !query
            .tags
            .iter()
            .all(|tag| record.tags.iter().any(|existing| existing == tag))
    {
        return false;
    }

    if !query.persons.is_empty()
        && !query
            .persons
            .iter()
            .all(|person| record.persons.iter().any(|existing| existing == person))
    {
        return false;
    }

    let in_range = timestamps_for_query(record, query.mode)
        .into_iter()
        .flatten()
        .any(|ts| ts >= query.start_at && ts <= query.end_at);

    if in_range {
        return true;
    }

    query.mode == AiSummaryMode::FutureTasks
        && record.record_type == RecordType::Task
        && matches!(
            record.status,
            Some(TaskStatus::Todo) | Some(TaskStatus::InProgress)
        )
        && record.completed_at.is_none()
}

fn timestamps_for_query(record: &Record, mode: AiSummaryMode) -> [Option<DateTime<Utc>>; 4] {
    match record.record_type {
        RecordType::Task => [
            Some(record.created_at),
            record.started_at,
            record.completed_at,
            if mode == AiSummaryMode::FutureTasks {
                Some(record.updated_at)
            } else {
                None
            },
        ],
        RecordType::Note | RecordType::Idea | RecordType::Event => {
            [Some(record.created_at), None, None, None]
        }
    }
}

fn primary_sort_timestamp(record: &Record) -> DateTime<Utc> {
    match record.record_type {
        RecordType::Task => record
            .completed_at
            .or(record.started_at)
            .unwrap_or(record.created_at),
        RecordType::Note | RecordType::Idea | RecordType::Event => record.created_at,
    }
}

fn render_day_block(date: NaiveDate, items: &[Record]) -> String {
    let mut lines = vec![format!("## {}", date.format("%Y-%m-%d"))];
    for record in items {
        lines.push(render_record_line(record));
    }
    lines.join("\n")
}

fn render_record_line(record: &Record) -> String {
    let local_time = primary_sort_timestamp(record)
        .with_timezone(&Local)
        .format("%H:%M")
        .to_string();
    let title = truncate_text(&record.display_title(), 80);
    let mut detail = truncate_text(&record.content.replace('\n', " "), 140);
    if detail == title {
        detail.clear();
    }

    let prefix = match record.record_type {
        RecordType::Task => match record.status {
            Some(TaskStatus::Done) => "完成任务",
            Some(TaskStatus::Cancelled) => "取消任务",
            Some(TaskStatus::InProgress) => "进行中任务",
            _ => "待办任务",
        },
        RecordType::Note => "记录",
        RecordType::Idea => "想法",
        RecordType::Event => "事件",
    };

    let mut line = format!("- [{} {}] {}", local_time, prefix, title);
    if !detail.is_empty() {
        line.push_str(" | ");
        line.push_str(&detail);
    }
    if !record.tags.is_empty() {
        line.push_str(" | 标签:");
        line.push_str(&record.tags.join(", "));
    }
    if !record.persons.is_empty() {
        line.push_str(" | 人物:");
        line.push_str(&record.persons.join(", "));
    }
    line
}

fn truncate_text(input: &str, limit: usize) -> String {
    let trimmed = input.trim();
    let mut chars = trimmed.chars();
    let preview = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn split_day_blocks(day_blocks: &[AiDayBlock]) -> Vec<Vec<AiDayBlock>> {
    if day_blocks.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut current_len = 0usize;

    for block in day_blocks {
        let block_len = block.rendered.len();
        if !current.is_empty() && current_len + block_len > TARGET_CHUNK_CHARS {
            chunks.push(current);
            current = Vec::new();
            current_len = 0;
        }
        current.push(block.clone());
        current_len += block_len;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn render_chunks(chunks: &[Vec<AiDayBlock>]) -> String {
    chunks
        .iter()
        .flat_map(|chunk| chunk.iter().map(|block| block.rendered.as_str()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn summary_system_prompt(mode: AiSummaryMode) -> &'static str {
    match mode {
        AiSummaryMode::PastSummary => {
            "你是用户的工作记录分析助手。你只能根据提供的上下文输出，不能虚构事实。\
             如果信息不足，必须明确写“上下文不足”。请用简洁中文输出，先写结论，再写支持这些结论的事实。\
             结论与建议必须分开；建议必须显式标注为“推断建议”。"
        }
        AiSummaryMode::FutureTasks => {
            "你是用户的工作梳理助手。你只能根据提供的上下文输出，不能虚构事实。\
             如果信息不足，必须明确写“上下文不足”。请用简洁中文输出，明确区分“已知事实”与“推断建议”。\
             重点是从已有工作与未完成事项中提炼下一步。"
        }
    }
}

fn chunk_system_prompt(mode: AiSummaryMode) -> &'static str {
    match mode {
        AiSummaryMode::PastSummary => {
            "请将这批工作记录压缩成结构化事实摘要。只保留：完成了什么、推进了什么、卡在哪里、涉及哪些标签/人物。不要写空话。"
        }
        AiSummaryMode::FutureTasks => {
            "请将这批工作记录压缩成结构化事实摘要。只保留：已有进展、仍未完成事项、明显风险、可作为下一步的线索。不要写空话。"
        }
    }
}

fn build_final_prompt(query: &AiContextQuery, bundle: &AiContextBundle, context: &str) -> String {
    let structure = match query.mode {
        AiSummaryMode::PastSummary => {
            "请严格按以下结构输出：\n\
             1. 总体概览\n\
             2. 关键完成\n\
             3. 进行中 / 阻塞\n\
             4. 标签与人物线索\n\
             5. 值得回顾的细节\n\
             6. 推断建议"
        }
        AiSummaryMode::FutureTasks => {
            "请严格按以下结构输出：\n\
             1. 接下来最值得推进的事项\n\
             2. 建议拆成的具体任务\n\
             3. 潜在风险或阻塞\n\
             4. 还缺什么信息\n\
             5. 已知事实依据\n\
             6. 推断建议"
        }
    };

    format!(
        "目标：{}\n\
         时间范围：{}\n\
         命中记录：{} 条，其中任务 {} 条、开放任务 {} 条、已完成任务 {} 条、记录/想法/事件 {} 条。\n\
         请只基于以下上下文总结，不要补充外部知识。\n\n\
         {}\n\n\
         上下文：\n{}",
        query.mode.prompt_goal(),
        bundle.date_span_label,
        bundle.records.len(),
        bundle.task_count,
        bundle.open_task_count,
        bundle.completed_task_count,
        bundle.note_like_count,
        structure,
        context
    )
}

fn build_chunk_prompt(
    query: &AiContextQuery,
    bundle: &AiContextBundle,
    chunk_index: usize,
    total_chunks: usize,
    chunk: &[AiDayBlock],
) -> String {
    format!(
        "目标：{}\n\
         时间范围：{}\n\
         当前是第 {}/{} 个上下文分块。\n\
         请把这部分内容压缩成紧凑的事实摘要，方便后续总汇总，不要输出结论性判断。\n\n{}",
        query.mode.prompt_goal(),
        bundle.date_span_label,
        chunk_index,
        total_chunks,
        render_chunks(&[chunk.to_vec()])
    )
}

fn build_merge_prompt(
    query: &AiContextQuery,
    bundle: &AiContextBundle,
    partials: &[String],
) -> String {
    build_final_prompt(query, bundle, &partials.join("\n\n"))
}

struct AiClient {
    client: reqwest::blocking::Client,
    settings: AiSettings,
    api_key: String,
}

impl AiClient {
    fn new(settings: &AiSettings, api_key: &str) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(settings.request_timeout_secs.max(10)))
            .build()
            .map_err(|err| format!("创建 AI 请求客户端失败: {err}"))?;
        Ok(Self {
            client,
            settings: settings.clone(),
            api_key: api_key.to_string(),
        })
    }

    fn request(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<String, String> {
        match self.settings.protocol {
            AiProviderProtocol::OpenAiCompatible => {
                self.request_openai_compatible(system_prompt, user_prompt)
            }
            AiProviderProtocol::Anthropic => {
                self.request_anthropic(system_prompt, user_prompt, max_tokens)
            }
        }
    }

    fn request_openai_compatible(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, String> {
        let endpoint = endpoint_url(&self.settings.normalized_base_url(), "chat/completions");
        let response = self
            .client
            .post(endpoint)
            .header(AUTHORIZATION, format!("Bearer {}", self.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "model": self.settings.model.trim(),
                "temperature": 0.2,
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ]
            }))
            .send()
            .map_err(|err| format!("请求 OpenAI 兼容接口失败: {err}"))?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|err| format!("读取 OpenAI 兼容响应失败: {err}"))?;
        if !status.is_success() {
            return Err(format!("OpenAI 兼容接口返回错误 {}: {}", status, body));
        }
        let value: Value =
            serde_json::from_str(&body).map_err(|err| format!("解析 OpenAI 响应失败: {err}"))?;
        extract_openai_text(&value)
    }

    fn request_anthropic(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<String, String> {
        let endpoint = endpoint_url(&self.settings.normalized_base_url(), "messages");
        let response = self
            .client
            .post(endpoint)
            .header("x-api-key", self.api_key.as_str())
            .header("anthropic-version", "2023-06-01")
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "model": self.settings.model.trim(),
                "system": system_prompt,
                "max_tokens": max_tokens,
                "temperature": 0.2,
                "messages": [
                    { "role": "user", "content": user_prompt }
                ]
            }))
            .send()
            .map_err(|err| format!("请求 Anthropic 接口失败: {err}"))?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|err| format!("读取 Anthropic 响应失败: {err}"))?;
        if !status.is_success() {
            return Err(format!("Anthropic 接口返回错误 {}: {}", status, body));
        }
        let value: Value =
            serde_json::from_str(&body).map_err(|err| format!("解析 Anthropic 响应失败: {err}"))?;
        extract_anthropic_text(&value)
    }
}

fn endpoint_url(base_url: &str, endpoint: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let endpoint = endpoint.trim_start_matches('/');
    if base.ends_with(endpoint) {
        base.to_string()
    } else {
        format!("{base}/{endpoint}")
    }
}

fn extract_openai_text(value: &Value) -> Result<String, String> {
    if let Some(content) = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
    {
        if let Some(text) = content.as_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        if let Some(items) = content.as_array() {
            let text = items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("content").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    Err("OpenAI 兼容响应中未找到可用文本".to_string())
}

fn extract_anthropic_text(value: &Value) -> Result<String, String> {
    let text = value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err("Anthropic 响应中未找到可用文本".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Priority, Record};
    use chrono::Duration;

    #[test]
    fn ai_settings_defaults_use_openai_shape_without_model() {
        let settings = AiSettings::default();
        assert_eq!(settings.protocol, AiProviderProtocol::OpenAiCompatible);
        assert_eq!(settings.normalized_base_url(), DEFAULT_OPENAI_BASE_URL);
        assert!(settings.model.is_empty());
        assert!(!settings.has_connection_config());
    }

    #[test]
    fn future_mode_keeps_open_tasks_outside_date_range() {
        let now = Utc::now();
        let mut open_task =
            Record::new_task("Task".to_string(), "Body".to_string(), Priority::High);
        open_task.created_at = now - Duration::days(10);
        open_task.updated_at = now - Duration::days(10);
        open_task.tags = vec!["work".to_string()];

        let query = AiContextQuery {
            start_at: now - Duration::days(2),
            end_at: now,
            tags: vec!["work".to_string()],
            persons: Vec::new(),
            mode: AiSummaryMode::FutureTasks,
        };

        let bundle = build_context_bundle(&[open_task], &query);
        assert_eq!(bundle.records.len(), 1);
        assert_eq!(bundle.open_task_count, 1);
    }

    #[test]
    fn split_day_blocks_produces_multiple_chunks_for_large_context() {
        let blocks = (0..4)
            .map(|idx| AiDayBlock {
                label: format!("2026-04-0{}", idx + 1),
                rendered: "a".repeat(TARGET_CHUNK_CHARS / 2),
            })
            .collect::<Vec<_>>();

        let chunks = split_day_blocks(&blocks);
        assert!(chunks.len() >= 2);
    }
}
