use crate::data_management::{
    default_export_file_name, AttachmentHealthSummary, AttachmentListItem,
    AttachmentStorageBackend, ConflictChoice, ConflictResolution, ImportConflict, ImportMode,
    ImportPreview, StorageUsageSummary,
};
use crate::file_dialog::{pick_archive_file, save_archive_file, ParentWindowHint};
use crate::models::{AttachmentStatus, RecordType};
use crate::store::Store;
use gpui::{prelude::*, *};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::v_flex;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
struct AttachmentPreviewEntry {
    item: AttachmentListItem,
    preview_image: Option<Arc<Image>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DataManagementPage {
    Overview,
    Attachments,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AttachmentFilter {
    All,
    Ready,
    Processing,
    Failed,
}

impl AttachmentFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "全部",
            Self::Ready => "可用",
            Self::Processing => "处理中",
            Self::Failed => "失败",
        }
    }

    fn matches(self, status: AttachmentStatus) -> bool {
        match self {
            Self::All => true,
            Self::Ready => status == AttachmentStatus::Ready,
            Self::Processing => status == AttachmentStatus::Processing,
            Self::Failed => status == AttachmentStatus::Failed,
        }
    }
}

pub struct DataManagementPanel {
    store: Store,
    page: DataManagementPage,
    attachment_filter: AttachmentFilter,
    storage_summary: Option<StorageUsageSummary>,
    attachment_health: Option<AttachmentHealthSummary>,
    attachments: Vec<AttachmentPreviewEntry>,
    overview_loading: bool,
    attachments_loading: bool,
    busy: bool,
    notice: Option<String>,
    error: Option<String>,
    import_preview: Option<ImportPreview>,
    import_mode: ImportMode,
    conflict_choices: HashMap<Uuid, ConflictChoice>,
}

impl DataManagementPanel {
    pub fn new(store: Store, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            store,
            page: DataManagementPage::Overview,
            attachment_filter: AttachmentFilter::All,
            storage_summary: None,
            attachment_health: None,
            attachments: Vec::new(),
            overview_loading: false,
            attachments_loading: false,
            busy: false,
            notice: None,
            error: None,
            import_preview: None,
            import_mode: ImportMode::ReplaceWithBackup,
            conflict_choices: HashMap::new(),
        };
        panel.load_overview(cx);
        panel
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load_overview(cx);
        if self.page == DataManagementPage::Attachments {
            self.load_attachments(cx);
        }
    }

    fn load_overview(&mut self, cx: &mut Context<Self>) {
        self.overview_loading = true;
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let storage = store.get_storage_usage_summary().await;
            let health = store.get_attachment_health_summary().await;
            let _ = view.update(cx, |this, cx| {
                this.overview_loading = false;
                match (storage, health) {
                    (Ok(storage_summary), Ok(attachment_health)) => {
                        this.storage_summary = Some(storage_summary);
                        this.attachment_health = Some(attachment_health);
                    }
                    (Err(err), _) | (_, Err(err)) => {
                        this.error = Some(err);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_attachments(&mut self, cx: &mut Context<Self>) {
        self.attachments_loading = true;
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let result = async {
                let items = store.get_all_attachments().await?;
                let mut entries = Vec::with_capacity(items.len());
                for item in items {
                    let preview_image = if item.attachment.status == AttachmentStatus::Ready {
                        match store.get_attachment_bytes(&item.attachment.id).await? {
                            Some(bytes) => {
                                preview_image_from_bytes(&item.attachment.mime_type, bytes)
                            }
                            None => None,
                        }
                    } else {
                        None
                    };
                    entries.push(AttachmentPreviewEntry {
                        item,
                        preview_image,
                    });
                }
                Ok::<_, String>(entries)
            }
            .await;

            let _ = view.update(cx, |this, cx| {
                this.attachments_loading = false;
                match result {
                    Ok(entries) => this.attachments = entries,
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_attachments_page(&mut self, cx: &mut Context<Self>) {
        self.page = DataManagementPage::Attachments;
        if self.attachments.is_empty() && !self.attachments_loading {
            self.load_attachments(cx);
        } else {
            cx.notify();
        }
    }

    fn return_to_overview(&mut self, cx: &mut Context<Self>) {
        self.page = DataManagementPage::Overview;
        cx.notify();
    }

    fn set_attachment_filter(&mut self, filter: AttachmentFilter, cx: &mut Context<Self>) {
        self.attachment_filter = filter;
        cx.notify();
    }

    fn pick_export_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        let hint = ParentWindowHint::from_window(window);
        let file_name = default_export_file_name();
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let Some(path) = save_archive_file(hint, &file_name).await else {
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
                return;
            };

            let result = store.export_data(path).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        this.notice = Some(format!(
                            "导出完成：{} 条记录，{} 个附件，文件已保存到 {}",
                            result.record_count,
                            result.attachment_count,
                            result.destination.display()
                        ));
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn pick_import_archive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        let hint = ParentWindowHint::from_window(window);
        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let Some(path) = pick_archive_file(hint).await else {
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
                return;
            };

            let result = store.preview_import(path).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(preview) => {
                        this.conflict_choices.clear();
                        this.import_mode = ImportMode::ReplaceWithBackup;
                        this.import_preview = Some(preview);
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_import_mode(&mut self, mode: ImportMode, cx: &mut Context<Self>) {
        self.import_mode = mode;
        cx.notify();
    }

    fn set_conflict_choice(
        &mut self,
        record_id: Uuid,
        choice: ConflictChoice,
        cx: &mut Context<Self>,
    ) {
        self.conflict_choices.insert(record_id, choice);
        cx.notify();
    }

    fn apply_import(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.import_preview.clone() else {
            return;
        };
        if self.import_mode == ImportMode::Merge
            && preview
                .conflicts
                .iter()
                .any(|conflict| !self.conflict_choices.contains_key(&conflict.record_id))
        {
            self.error = Some("请先为所有冲突记录选择保留本地还是使用导入数据".to_string());
            cx.notify();
            return;
        }

        self.busy = true;
        self.error = None;
        self.notice = None;
        let store = self.store.clone();
        let archive_path = preview.archive_path.clone();
        let mode = self.import_mode;
        let resolutions = preview
            .conflicts
            .iter()
            .filter_map(|conflict| {
                self.conflict_choices
                    .get(&conflict.record_id)
                    .copied()
                    .map(|choice| ConflictResolution {
                        record_id: conflict.record_id,
                        choice,
                    })
            })
            .collect::<Vec<_>>();

        cx.spawn(async move |view, cx| {
            let result = store.apply_import(archive_path, mode, resolutions).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        let backup_message = result
                            .backup_path
                            .as_ref()
                            .map(|path| format!("，已备份到 {}", path.display()))
                            .unwrap_or_default();
                        this.notice = Some(format!(
                            "导入完成：{} 条记录，{} 个附件{}",
                            result.imported_record_count,
                            result.imported_attachment_count,
                            backup_message
                        ));
                        this.import_preview = None;
                        this.conflict_choices.clear();
                        this.load_overview(cx);
                        if this.page == DataManagementPage::Attachments {
                            this.load_attachments(cx);
                        }
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_attachment(&mut self, entry: AttachmentPreviewEntry, cx: &mut Context<Self>) {
        if entry.item.attachment.status != AttachmentStatus::Ready {
            self.error = Some("只有可用附件才能预览".to_string());
            cx.notify();
            return;
        }

        let store = self.store.clone();
        cx.spawn(async move |view, cx| {
            let result = store.get_attachment_bytes(&entry.item.attachment.id).await;
            let _ = view.update(cx, |this, cx| {
                match result {
                    Ok(bytes) => {
                        if let Err(err) = crate::system_preview::open_saved_attachment(
                            &entry.item.attachment,
                            bytes,
                        ) {
                            this.error = Some(err);
                        }
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn filtered_attachments(&self) -> Vec<AttachmentPreviewEntry> {
        self.attachments
            .iter()
            .filter(|entry| self.attachment_filter.matches(entry.item.attachment.status))
            .cloned()
            .collect()
    }

    fn render_overview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let summary = self.storage_summary.clone().unwrap_or_default();
        let health = self.attachment_health.clone().unwrap_or_default();
        let has_attachment_issues = health.processing_count > 0 || health.failed_count > 0;
        let import_ready = self.import_preview.is_some();
        let apply_enabled = self.import_preview.as_ref().is_some_and(|preview| {
            self.import_mode == ImportMode::ReplaceWithBackup
                || preview
                    .conflicts
                    .iter()
                    .all(|conflict| self.conflict_choices.contains_key(&conflict.record_id))
        });

        v_flex()
            .gap(px(20.0))
            .child(
                v_flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x262626))
                            .child("数据管理"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8c8c8c))
                            .line_height(relative(1.5))
                            .child("查看当前本地数据体积、附件健康状态，并执行数据导入导出。"),
                    ),
            )
            .when(self.overview_loading, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x8c8c8c))
                        .child("正在读取数据统计…"),
                )
            })
            .when(has_attachment_issues, |this| {
                this.child(
                    div()
                        .p(px(14.0))
                        .rounded(px(12.0))
                        .bg(rgb(0xfff7e6))
                        .border_1()
                        .border_color(rgb(0xffd591))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xad6800))
                                .line_height(relative(1.5))
                                .child(format!(
                                    "当前有 {} 个处理中附件、{} 个失败附件，建议进入附件列表排查。",
                                    health.processing_count, health.failed_count
                                )),
                        ),
                )
            })
            .when(self.notice.is_some(), |this| {
                this.child(render_message_box(
                    self.notice.as_deref().unwrap_or_default(),
                    false,
                ))
            })
            .when(self.error.is_some(), |this| {
                this.child(render_message_box(
                    self.error.as_deref().unwrap_or_default(),
                    true,
                ))
            })
            .child(render_card(
                "容量概览",
                "按业务数据逻辑体积统计，不包含 SQLite 页碎片和 WAL 文件。",
                vec![
                    render_stat_block("文本占用", &format_bytes(summary.text_bytes)),
                    render_stat_block("附件占用", &format_bytes(summary.attachment_bytes)),
                    render_stat_block("总占用", &format_bytes(summary.total_bytes)),
                    render_stat_block(
                        "数量概览",
                        &format!(
                            "{} 条记录 · {} 个附件",
                            summary.record_count, summary.attachment_count
                        ),
                    ),
                ],
            ))
            .child(render_card(
                "附件健康",
                "可用、处理中、失败状态直接来自附件表状态字段。",
                vec![
                    render_stat_block("可用附件", &health.ready_count.to_string()),
                    render_stat_block("处理中附件", &health.processing_count.to_string()),
                    render_stat_block("失败附件", &health.failed_count.to_string()),
                    self.render_inline_action_button(
                        "open-attachment-list",
                        "查看附件列表",
                        true,
                        !self.busy,
                        cx.listener(|this, _event: &ClickEvent, _window, cx| {
                            this.open_attachments_page(cx);
                        }),
                    ),
                ],
            ))
            .child(render_card(
                "导入导出",
                "导出会生成单个归档文件；替换导入会先自动备份当前数据。",
                vec![
                    div()
                        .flex()
                        .gap(px(10.0))
                        .child(self.render_inline_action_button(
                            "export-data",
                            "导出数据",
                            true,
                            !self.busy,
                            cx.listener(|this, _event, window, cx| {
                                this.pick_export_destination(window, cx);
                            }),
                        ))
                        .child(self.render_inline_action_button(
                            "pick-import-archive",
                            "选择导入包",
                            false,
                            !self.busy,
                            cx.listener(|this, _event, window, cx| {
                                this.pick_import_archive(window, cx);
                            }),
                        ))
                        .into_any_element(),
                    if import_ready {
                        self.render_import_preview(cx).into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(0x8c8c8c))
                            .child("尚未选择导入包。")
                            .into_any_element()
                    },
                    div()
                        .flex()
                        .justify_end()
                        .child(self.render_inline_action_button(
                            "apply-import",
                            "开始导入",
                            true,
                            !self.busy && apply_enabled && import_ready,
                            cx.listener(|this, _event, _window, cx| {
                                this.apply_import(cx);
                            }),
                        ))
                        .into_any_element(),
                ],
            ))
    }

    fn render_import_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = self.import_preview.clone().unwrap();
        v_flex()
            .gap(px(12.0))
            .p(px(14.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(0xf0f0f0))
            .bg(rgb(0xfcfcfc))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x262626))
                    .child(format!(
                        "导入预检：{} 条记录 · {} 个附件",
                        preview.record_count, preview.attachment_count
                    )),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .line_height(relative(1.5))
                    .child(format!(
                        "可用 {} · 处理中 {} · 失败 {} · 冲突 {}",
                        preview.ready_attachment_count,
                        preview.processing_attachment_count,
                        preview.failed_attachment_count,
                        preview.conflicts.len()
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(self.render_inline_action_button(
                        "mode-replace-with-backup",
                        "备份后替换",
                        self.import_mode == ImportMode::ReplaceWithBackup,
                        !self.busy,
                        cx.listener(|this, _event: &ClickEvent, _window, cx| {
                            this.set_import_mode(ImportMode::ReplaceWithBackup, cx);
                        }),
                    ))
                    .child(self.render_inline_action_button(
                        "mode-merge",
                        "合并导入",
                        self.import_mode == ImportMode::Merge,
                        !self.busy,
                        cx.listener(|this, _event, _window, cx| {
                            this.set_import_mode(ImportMode::Merge, cx);
                        }),
                    )),
            )
            .when(
                self.import_mode == ImportMode::Merge && !preview.conflicts.is_empty(),
                |this| {
                    this.child(v_flex().gap(px(10.0)).children(
                        preview.conflicts.into_iter().map(|conflict| {
                            self.render_conflict_row(conflict, cx).into_any_element()
                        }),
                    ))
                },
            )
    }

    fn render_conflict_row(
        &self,
        conflict: ImportConflict,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let choice = self.conflict_choices.get(&conflict.record_id).copied();
        v_flex()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(0xf0f0f0))
            .bg(rgb(0xffffff))
            .child(
                div().flex().justify_between().gap(px(16.0)).child(
                    v_flex()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x262626))
                                .child(conflict.display_title.clone()),
                        )
                        .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(format!(
                            "{} · 本地更新 {} · 导入更新 {}",
                            record_type_label(conflict.record_type),
                            format_time(conflict.local_updated_at),
                            format_time(conflict.imported_updated_at)
                        ))),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(self.render_inline_action_button(
                        format!("conflict-keep-local-{}", conflict.record_id),
                        "保留本地",
                        choice == Some(ConflictChoice::KeepLocal),
                        !self.busy,
                        cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                            this.set_conflict_choice(
                                conflict.record_id,
                                ConflictChoice::KeepLocal,
                                cx,
                            );
                        }),
                    ))
                    .child(self.render_inline_action_button(
                        format!("conflict-use-import-{}", conflict.record_id),
                        "使用导入",
                        choice == Some(ConflictChoice::UseImported),
                        !self.busy,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_conflict_choice(
                                conflict.record_id,
                                ConflictChoice::UseImported,
                                cx,
                            );
                        }),
                    )),
            )
    }

    fn render_attachments_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filtered_attachments();
        let payload_bytes = filtered
            .iter()
            .map(|entry| entry.item.payload_bytes as u64)
            .sum::<u64>();
        let issue_count = filtered
            .iter()
            .filter(|entry| {
                matches!(
                    entry.item.attachment.status,
                    AttachmentStatus::Processing | AttachmentStatus::Failed
                )
            })
            .count();

        v_flex()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        v_flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x262626))
                                    .child("数据管理 / 附件列表"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x8c8c8c))
                                    .line_height(relative(1.5))
                                    .child("集中查看所有附件资产、状态和存储后端。"),
                            ),
                    )
                    .child(self.render_inline_action_button(
                        "back-to-overview",
                        "返回数据管理",
                        false,
                        !self.busy,
                        cx.listener(|this, _event: &ClickEvent, _window, cx| {
                            this.return_to_overview(cx);
                        }),
                    )),
            )
            .when(self.error.is_some(), |this| {
                this.child(render_message_box(
                    self.error.as_deref().unwrap_or_default(),
                    true,
                ))
            })
            .child(
                div().flex().gap(px(8.0)).children(
                    [
                        AttachmentFilter::All,
                        AttachmentFilter::Ready,
                        AttachmentFilter::Processing,
                        AttachmentFilter::Failed,
                    ]
                    .into_iter()
                    .map(|filter| {
                        self.render_inline_action_button(
                            format!("attachment-filter-{}", filter.label()),
                            filter.label(),
                            self.attachment_filter == filter,
                            !self.busy,
                            cx.listener(move |this, _event, _window, cx| {
                                this.set_attachment_filter(filter, cx);
                            }),
                        )
                        .into_any_element()
                    }),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(render_stat_block("当前结果", &filtered.len().to_string()))
                    .child(render_stat_block("当前占用", &format_bytes(payload_bytes)))
                    .child(render_stat_block("异常附件", &issue_count.to_string())),
            )
            .when(self.attachments_loading, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x8c8c8c))
                        .child("正在加载附件列表…"),
                )
            })
            .child(
                div().flex().flex_col().gap(px(10.0)).children(
                    filtered
                        .into_iter()
                        .map(|entry| self.render_attachment_row(entry, cx).into_any_element()),
                ),
            )
    }

    fn render_attachment_row(
        &self,
        entry: AttachmentPreviewEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let status_text = attachment_status_label(entry.item.attachment.status);
        let backend_text = attachment_backend_label(entry.item.storage_backend);
        let preview_entry = entry.clone();

        div()
            .flex()
            .gap(px(14.0))
            .p(px(14.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(0xf0f0f0))
            .bg(rgb(0xfcfcfc))
            .child(render_attachment_preview(&entry))
            .child(
                v_flex()
                    .flex_1()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x262626))
                            .child(entry.item.attachment.file_name.clone()),
                    )
                    .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(format!(
                        "{} · {}",
                        record_type_label(entry.item.record_type),
                        entry.item.record_title
                    )))
                    .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(format!(
                        "{} · {} · {} × {} · {}",
                        status_text,
                        entry.item.attachment.mime_type,
                        entry.item.attachment.width,
                        entry.item.attachment.height,
                        format_bytes(entry.item.payload_bytes as u64)
                    )))
                    .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(format!(
                        "{} · {}",
                        backend_text,
                        format_time(entry.item.attachment.created_at)
                    )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x8c8c8c))
                            .child(entry.item.attachment.file_path.clone()),
                    )
                    .when(entry.item.attachment.error_message.is_some(), |this| {
                        this.child(
                            div().text_xs().text_color(rgb(0xff4d4f)).child(
                                entry
                                    .item
                                    .attachment
                                    .error_message
                                    .clone()
                                    .unwrap_or_default(),
                            ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.render_inline_action_button(
                                format!("preview-attachment-{}", entry.item.attachment.id),
                                "系统预览",
                                false,
                                !self.busy
                                    && entry.item.attachment.status == AttachmentStatus::Ready,
                                cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                    this.open_attachment(preview_entry.clone(), cx);
                                }),
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x8c8c8c))
                                    .child("失败附件暂不支持重试，后续版本补充。"),
                            ),
                    ),
            )
    }

    fn render_inline_action_button<F>(
        &self,
        id: impl Into<String>,
        label: &'static str,
        selected: bool,
        enabled: bool,
        on_click: F,
    ) -> AnyElement
    where
        F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    {
        let button = Button::new(id.into())
            .child(label)
            .when(selected, |button| {
                button.with_variant(gpui_component::button::ButtonVariant::Primary)
            })
            .when(!enabled, |button| button.text_color(rgb(0x8c8c8c)));

        if enabled {
            button.on_click(on_click).into_any_element()
        } else {
            button.into_any_element()
        }
    }
}

impl Render for DataManagementPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .overflow_y_scrollbar()
            .pr(px(16.0))
            .child(match self.page {
                DataManagementPage::Overview => self.render_overview(cx).into_any_element(),
                DataManagementPage::Attachments => {
                    self.render_attachments_page(cx).into_any_element()
                }
            })
    }
}

fn render_card(
    title: &'static str,
    description: &'static str,
    children: Vec<AnyElement>,
) -> AnyElement {
    v_flex()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgb(0xf0f0f0))
        .bg(rgb(0xffffff))
        .child(
            v_flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x262626))
                        .child(title),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x8c8c8c))
                        .line_height(relative(1.5))
                        .child(description),
                ),
        )
        .children(children)
        .into_any_element()
}

fn render_stat_block(label: &'static str, value: &str) -> AnyElement {
    v_flex()
        .gap(px(4.0))
        .min_w(px(120.0))
        .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(label))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x262626))
                .child(value.to_string()),
        )
        .into_any_element()
}

fn render_message_box(message: &str, is_error: bool) -> AnyElement {
    let (background, border, color) = if is_error {
        (rgb(0xfff2f0), rgb(0xffccc7), rgb(0xcf1322))
    } else {
        (rgb(0xf6ffed), rgb(0xb7eb8f), rgb(0x389e0d))
    };

    div()
        .p(px(12.0))
        .rounded(px(10.0))
        .bg(background)
        .border_1()
        .border_color(border)
        .child(
            div()
                .text_sm()
                .text_color(color)
                .line_height(relative(1.5))
                .child(message.to_string()),
        )
        .into_any_element()
}

fn render_attachment_preview(entry: &AttachmentPreviewEntry) -> AnyElement {
    match entry.item.attachment.status {
        AttachmentStatus::Ready => entry
            .preview_image
            .clone()
            .map(|image| {
                div()
                    .w(px(96.0))
                    .h(px(96.0))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .bg(rgb(0xf5f5f5))
                    .child(img(image).w_full().h_full())
                    .into_any_element()
            })
            .unwrap_or_else(|| {
                div()
                    .w(px(96.0))
                    .h(px(96.0))
                    .rounded(px(10.0))
                    .bg(rgb(0xf5f5f5))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(rgb(0x8c8c8c))
                    .child("无预览")
                    .into_any_element()
            }),
        AttachmentStatus::Processing => div()
            .w(px(96.0))
            .h(px(96.0))
            .rounded(px(10.0))
            .bg(rgb(0xf5f5f5))
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(rgb(0x8c8c8c))
            .child("处理中")
            .into_any_element(),
        AttachmentStatus::Failed => div()
            .w(px(96.0))
            .h(px(96.0))
            .rounded(px(10.0))
            .bg(rgb(0xfff2f0))
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(rgb(0xff4d4f))
            .child("失败")
            .into_any_element(),
    }
}

fn preview_image_from_bytes(mime_type: &str, bytes: Vec<u8>) -> Option<Arc<Image>> {
    let format = ImageFormat::from_mime_type(mime_type)?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_time(time: chrono::DateTime<chrono::Utc>) -> String {
    time.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn record_type_label(record_type: RecordType) -> &'static str {
    match record_type {
        RecordType::Task => "任务",
        RecordType::Note => "记录",
        RecordType::Event => "事件",
        RecordType::Idea => "想法",
    }
}

fn attachment_status_label(status: AttachmentStatus) -> &'static str {
    match status {
        AttachmentStatus::Ready => "可用",
        AttachmentStatus::Processing => "处理中",
        AttachmentStatus::Failed => "失败",
    }
}

fn attachment_backend_label(backend: AttachmentStorageBackend) -> &'static str {
    match backend {
        AttachmentStorageBackend::DatabaseBlob => "数据库内",
        AttachmentStorageBackend::FilePathFallback => "路径回退",
        AttachmentStorageBackend::NoPayload => "无可用内容",
    }
}
