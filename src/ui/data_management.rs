use crate::data_management::{
    app_data_dir, default_export_file_name, AttachmentHealthSummary, AttachmentListItem,
    AttachmentStorageBackend, ConflictChoice, ConflictResolution, ImportConflict, ImportMode,
    ImportPreview, StorageUsageSummary,
};
use crate::git_sync::{GitRemoteSyncConfig, GitRemoteSyncMetadata, GitRemoteVerification};
use crate::models::{AttachmentStatus, RecordType};
use crate::platform::{
    open_saved_attachment, pick_archive_file, save_archive_file, ParentWindowHint,
};
use crate::settings::{
    load_app_settings, save_app_settings, settings_file_path, DataSettings, ImportModePreference,
};
use crate::store::{GitRemoteSyncPullPreview, Store};
use gpui::{prelude::*, *};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::v_flex;
use gpui_component::Sizable;
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
    default_import_mode: ImportModePreference,
    git_sync_config: GitRemoteSyncConfig,
    git_sync_verification: Option<GitRemoteVerification>,
    remote_url_input: Entity<InputState>,
    branch_input: Entity<InputState>,
    base_path_input: Entity<InputState>,
    pending_import_remote_commit: Option<String>,
    pending_import_remote_metadata: Option<GitRemoteSyncMetadata>,
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
    pub fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let remote_url_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("git@host:owner/repo.git"));
        let branch_input = cx.new(|cx| InputState::new(window, cx).placeholder("main"));
        let base_path_input = cx.new(|cx| InputState::new(window, cx).placeholder("robinne-sync"));
        let mut panel = Self {
            store,
            page: DataManagementPage::Overview,
            attachment_filter: AttachmentFilter::All,
            default_import_mode: ImportModePreference::ReplaceWithBackup,
            git_sync_config: GitRemoteSyncConfig::default(),
            git_sync_verification: None,
            remote_url_input,
            branch_input,
            base_path_input,
            pending_import_remote_commit: None,
            pending_import_remote_metadata: None,
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
        panel.load_settings_state(window, cx);
        panel
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load_overview(cx);
        if self.page == DataManagementPage::Attachments {
            self.load_attachments(cx);
        }
    }

    pub fn reload_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_settings_state(window, cx);
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

    fn load_settings_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match load_app_settings() {
            Ok(settings) => {
                self.default_import_mode = settings.data.default_import_mode;
                self.import_mode = self.default_import_mode.to_import_mode();
                self.git_sync_config = settings.git_sync.normalized();
                self.git_sync_verification = None;
                let config = self.git_sync_config.clone();
                self.apply_git_sync_inputs(&config, window, cx);
            }
            Err(err) => {
                self.error = Some(err);
                cx.notify();
            }
        }
    }

    fn apply_git_sync_inputs(
        &mut self,
        config: &GitRemoteSyncConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_url_input.update(cx, |input, cx| {
            input.set_value(&config.remote_url, window, cx);
        });
        self.branch_input.update(cx, |input, cx| {
            input.set_value(&config.branch, window, cx);
        });
        self.base_path_input.update(cx, |input, cx| {
            input.set_value(&config.base_path, window, cx);
        });
    }

    fn current_git_sync_config(&self, cx: &App) -> GitRemoteSyncConfig {
        let mut config = self.git_sync_config.clone();
        config.remote_url = self.remote_url_input.read(cx).text().to_string();
        config.branch = self.branch_input.read(cx).text().to_string();
        config.base_path = self.base_path_input.read(cx).text().to_string();
        config.normalized()
    }

    fn save_git_sync_config(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        let store = self.store.clone();
        let config = self.current_git_sync_config(cx);
        cx.spawn(async move |view, cx| {
            let result = store.save_git_remote_sync_config(config).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(state) => {
                        this.git_sync_config = state.config;
                        this.notice = Some("已保存 Git 远端同步配置".to_string());
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_default_import_mode(
        &mut self,
        preference: ImportModePreference,
        cx: &mut Context<Self>,
    ) {
        match load_app_settings() {
            Ok(mut settings) => {
                settings.data = DataSettings {
                    default_import_mode: preference,
                };
                match save_app_settings(&settings) {
                    Ok(_) => {
                        self.default_import_mode = preference;
                        self.import_mode = preference.to_import_mode();
                        self.notice = Some("已保存默认导入策略".to_string());
                        self.error = None;
                    }
                    Err(err) => {
                        self.error = Some(err);
                        self.notice = None;
                    }
                }
            }
            Err(err) => {
                self.error = Some(err);
                self.notice = None;
            }
        }
        cx.notify();
    }

    fn verify_git_remote(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        let store = self.store.clone();
        let config = self.current_git_sync_config(cx);
        cx.spawn(async move |view, cx| {
            let result = store.verify_git_remote(config.clone()).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(verification) => {
                        this.git_sync_verification = Some(verification.clone());
                        this.git_sync_config = config;
                        this.notice = Some("已验证 Git 远端连接".to_string());
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn push_to_git_remote(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        let store = self.store.clone();
        let config = self.current_git_sync_config(cx);
        cx.spawn(async move |view, cx| {
            let result = store.push_snapshot_to_git_remote(config.clone()).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(result) => {
                        let mut updated = config.clone();
                        updated.last_seen_remote_commit = Some(result.remote_commit.clone());
                        updated.last_sync_at = Some(chrono::Utc::now());
                        this.git_sync_config = updated;
                        this.pending_import_remote_commit = None;
                        this.pending_import_remote_metadata = None;
                        this.git_sync_verification = Some(GitRemoteVerification {
                            remote_url: config.remote_url.clone(),
                            branch: config.branch.clone(),
                            git_version: "git".to_string(),
                            remote_head_commit: Some(result.remote_commit.clone()),
                            remote_metadata: Some(result.metadata.clone()),
                        });
                        this.notice = Some(format!(
                            "已推送到 Git 远端：{} 条记录，{} 个附件",
                            result.metadata.record_count, result.metadata.attachment_count
                        ));
                    }
                    Err(err) => this.error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn pull_from_git_remote(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.error = None;
        self.notice = None;
        let store = self.store.clone();
        let config = self.current_git_sync_config(cx);
        cx.spawn(async move |view, cx| {
            let result = store.pull_snapshot_from_git_remote(config.clone()).await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(GitRemoteSyncPullPreview {
                        preview,
                        remote_commit,
                        metadata,
                    }) => {
                        this.conflict_choices.clear();
                        this.import_mode = this.default_import_mode.to_import_mode();
                        this.import_preview = Some(preview);
                        this.pending_import_remote_commit = Some(remote_commit);
                        this.pending_import_remote_metadata = Some(metadata.clone());
                        this.git_sync_config = config;
                        this.notice = Some("已从 Git 远端拉取快照，请确认导入策略".to_string());
                    }
                    Err(err) => this.error = Some(err),
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
                        this.import_mode = this.default_import_mode.to_import_mode();
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
        let pending_sync_config = self
            .pending_import_remote_commit
            .as_ref()
            .map(|remote_commit| {
                let mut config = self.current_git_sync_config(cx);
                config.last_seen_remote_commit = Some(remote_commit.clone());
                config.last_sync_at = Some(chrono::Utc::now());
                config
            });
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
            let sync_save_result = if result.is_ok() {
                if let Some(config) = pending_sync_config {
                    Some(store.save_git_remote_sync_config(config).await)
                } else {
                    None
                }
            } else {
                None
            };
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
                        if let Some(Err(err)) = sync_save_result.as_ref() {
                            this.notice =
                                Some(format!("导入已完成，但未能更新 Git 同步状态：{}", err));
                        }
                        if let Some(Ok(state)) = sync_save_result {
                            this.git_sync_config = state.config;
                        }
                        this.import_preview = None;
                        this.conflict_choices.clear();
                        this.pending_import_remote_commit = None;
                        this.pending_import_remote_metadata = None;
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
                        if let Err(err) = open_saved_attachment(&entry.item.attachment, bytes) {
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
                            .child("数据与同步"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x8c8c8c))
                            .line_height(relative(1.5))
                            .child("查看本地数据体积、导入导出状态，并管理 Git 远端同步。"),
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
                "默认导入策略",
                "选择导入包后会默认使用这里的策略，你仍然可以在导入预检里临时切换。",
                vec![div()
                    .flex()
                    .gap(px(10.0))
                    .child(self.render_inline_action_button(
                        "default-import-replace",
                        "备份后替换",
                        self.default_import_mode == ImportModePreference::ReplaceWithBackup,
                        !self.busy,
                        cx.listener(|this, _event: &ClickEvent, _window, cx| {
                            this.save_default_import_mode(
                                ImportModePreference::ReplaceWithBackup,
                                cx,
                            );
                        }),
                    ))
                    .child(self.render_inline_action_button(
                        "default-import-merge",
                        "合并导入",
                        self.default_import_mode == ImportModePreference::Merge,
                        !self.busy,
                        cx.listener(|this, _event, _window, cx| {
                            this.save_default_import_mode(ImportModePreference::Merge, cx);
                        }),
                    ))
                    .into_any_element()],
            ))
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
            .child(render_card(
                "本地路径",
                "这些路径用于保存数据库、设置和导入备份。",
                vec![
                    render_stat_block("数据目录", &app_data_dir().display().to_string()),
                    render_stat_block(
                        "数据库文件",
                        &app_data_dir().join("data.db").display().to_string(),
                    ),
                    render_stat_block("设置文件", &settings_file_path().display().to_string()),
                ],
            ))
            .child(self.render_git_sync_card(cx))
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

    fn render_git_sync_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let verification = self.git_sync_verification.clone();
        let last_sync = self
            .git_sync_config
            .last_sync_at
            .map(format_time)
            .unwrap_or_else(|| "尚未同步".to_string());
        let remote_commit = verification
            .as_ref()
            .and_then(|value| value.remote_head_commit.clone())
            .or_else(|| self.git_sync_config.last_seen_remote_commit.clone())
            .unwrap_or_else(|| "未知".to_string());
        let metadata_text = verification
            .as_ref()
            .and_then(|value| value.remote_metadata.as_ref())
            .map(|metadata| {
                format!(
                    "{} 条记录 · {} 个附件 · 导出于 {}",
                    metadata.record_count,
                    metadata.attachment_count,
                    format_time(metadata.exported_at)
                )
            })
            .unwrap_or_else(|| "远端尚未读取到同步元信息".to_string());

        render_card(
            "Git 远端同步",
            "通过本机 Git 与任意兼容的远端仓库进行手动快照同步。建议提前配置 SSH 或凭据助手，应用不会托管账号密码。",
            vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(render_input_row("远端仓库", &self.remote_url_input))
                    .child(render_input_row("分支", &self.branch_input))
                    .child(render_input_row("同步目录", &self.base_path_input))
                    .into_any_element(),
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(self.render_compact_inline_action_button(
                        "save-git-sync-config",
                        "保存配置",
                        false,
                        !self.busy,
                        cx.listener(|this, _event: &ClickEvent, _window, cx| {
                            this.save_git_sync_config(cx);
                        }),
                    ))
                    .child(self.render_compact_inline_action_button(
                        "verify-git-remote",
                        "校验远端",
                        false,
                        !self.busy,
                        cx.listener(|this, _event, _window, cx| {
                            this.verify_git_remote(cx);
                        }),
                    ))
                    .child(self.render_compact_inline_action_button(
                        "push-git-remote",
                        "推送到远端",
                        true,
                        !self.busy,
                        cx.listener(|this, _event, _window, cx| {
                            this.push_to_git_remote(cx);
                        }),
                    ))
                    .child(self.render_compact_inline_action_button(
                        "pull-git-remote",
                        "从远端拉取",
                        false,
                        !self.busy,
                        cx.listener(|this, _event, _window, cx| {
                            this.pull_from_git_remote(cx);
                        }),
                    ))
                    .into_any_element(),
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(render_stat_block("最近同步", &last_sync))
                    .child(render_stat_block(
                        "远端版本",
                        &truncate_middle(&remote_commit, 18),
                    ))
                    .into_any_element(),
                div()
                    .text_sm()
                    .text_color(rgb(0x8c8c8c))
                    .line_height(relative(1.5))
                    .child(metadata_text)
                    .into_any_element(),
            ],
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

    fn render_compact_inline_action_button<F>(
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
            .small()
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

fn render_input_row(label: &'static str, input: &Entity<InputState>) -> AnyElement {
    v_flex()
        .gap(px(6.0))
        .child(div().text_xs().text_color(rgb(0x8c8c8c)).child(label))
        .child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgb(0xf0f0f0))
                .bg(rgb(0xffffff))
                .child(Input::new(input)),
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

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    let head = max_chars / 2 - 1;
    let tail = max_chars.saturating_sub(head + 1);
    format!(
        "{}…{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>()
    )
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
