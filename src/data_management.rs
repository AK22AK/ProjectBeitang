use crate::db::Database;
use crate::models::{Attachment, AttachmentStatus, Person, Record, RecordType, Tag};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const EXPORT_FORMAT_VERSION: u32 = 1;
const MANIFEST_PATH: &str = "manifest.json";

#[derive(Debug, Clone, Default)]
pub struct StorageUsageSummary {
    pub text_bytes: u64,
    pub attachment_bytes: u64,
    pub total_bytes: u64,
    pub record_count: usize,
    pub tag_count: usize,
    pub person_count: usize,
    pub attachment_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AttachmentHealthSummary {
    pub ready_count: usize,
    pub processing_count: usize,
    pub failed_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentStorageBackend {
    DatabaseBlob,
    FilePathFallback,
    NoPayload,
}

#[derive(Debug, Clone)]
pub struct AttachmentListItem {
    pub attachment: Attachment,
    pub record_title: String,
    pub record_type: RecordType,
    pub payload_bytes: usize,
    pub storage_backend: AttachmentStorageBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRecord {
    pub id: Uuid,
    pub title: Option<String>,
    pub content: String,
    pub priority: Option<crate::models::Priority>,
    pub status: Option<crate::models::TaskStatus>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub due_date: Option<DateTime<Utc>>,
    pub notified_at: Option<DateTime<Utc>>,
    pub cancelled_reason: Option<String>,
    pub record_type: RecordType,
    pub tags: Vec<String>,
    pub persons: Vec<String>,
}

impl ExportRecord {
    pub fn from_record(record: &Record) -> Self {
        Self {
            id: record.id,
            title: record.title.clone(),
            content: record.content.clone(),
            priority: record.priority.clone(),
            status: record.status.clone(),
            created_at: record.created_at,
            updated_at: record.updated_at,
            completed_at: record.completed_at,
            scheduled_for: record.scheduled_for,
            due_date: record.due_date,
            notified_at: record.notified_at,
            cancelled_reason: record.cancelled_reason.clone(),
            record_type: record.record_type.clone(),
            tags: record.tags.clone(),
            persons: record.persons.clone(),
        }
    }

    pub fn to_record(&self) -> Record {
        Record {
            id: self.id,
            title: self.title.clone(),
            content: self.content.clone(),
            priority: self.priority.clone(),
            status: self.status.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            completed_at: self.completed_at,
            scheduled_for: self.scheduled_for,
            due_date: self.due_date,
            notified_at: self.notified_at,
            cancelled_reason: self.cancelled_reason.clone(),
            record_type: self.record_type.clone(),
            tags: self.tags.clone(),
            persons: self.persons.clone(),
        }
    }

    pub fn display_title(&self) -> String {
        self.to_record().display_title()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportAttachmentEntry {
    pub id: String,
    pub record_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub created_at: DateTime<Utc>,
    pub status: AttachmentStatus,
    pub error_message: Option<String>,
    pub has_payload: bool,
}

impl ExportAttachmentEntry {
    pub fn from_attachment(attachment: &Attachment, has_payload: bool) -> Self {
        Self {
            id: attachment.id.clone(),
            record_id: attachment.record_id.clone(),
            file_name: attachment.file_name.clone(),
            file_path: attachment.file_path.clone(),
            file_size: attachment.file_size,
            mime_type: attachment.mime_type.clone(),
            width: attachment.width,
            height: attachment.height,
            created_at: attachment.created_at,
            status: attachment.status,
            error_message: attachment.error_message.clone(),
            has_payload,
        }
    }

    pub fn to_attachment(&self) -> Attachment {
        Attachment {
            id: self.id.clone(),
            record_id: self.record_id.clone(),
            file_name: self.file_name.clone(),
            file_path: self.file_path.clone(),
            file_size: self.file_size,
            mime_type: self.mime_type.clone(),
            width: self.width,
            height: self.height,
            created_at: self.created_at,
            status: self.status,
            error_message: self.error_message.clone(),
            source_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifestV1 {
    pub format_version: u32,
    pub schema_version: i64,
    pub exported_at: DateTime<Utc>,
    pub app_version: String,
    pub records: Vec<ExportRecord>,
    pub tags: Vec<Tag>,
    pub persons: Vec<Person>,
    pub attachments: Vec<ExportAttachmentEntry>,
}

#[derive(Debug, Clone)]
pub struct ExportResult {
    pub destination: PathBuf,
    pub record_count: usize,
    pub attachment_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    ReplaceWithBackup,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    KeepLocal,
    UseImported,
}

#[derive(Debug, Clone)]
pub struct ConflictResolution {
    pub record_id: Uuid,
    pub choice: ConflictChoice,
}

#[derive(Debug, Clone)]
pub struct ImportConflict {
    pub record_id: Uuid,
    pub display_title: String,
    pub record_type: RecordType,
    pub local_updated_at: DateTime<Utc>,
    pub imported_updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub archive_path: PathBuf,
    pub record_count: usize,
    pub tag_count: usize,
    pub person_count: usize,
    pub attachment_count: usize,
    pub ready_attachment_count: usize,
    pub processing_attachment_count: usize,
    pub failed_attachment_count: usize,
    pub conflicts: Vec<ImportConflict>,
}

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub backup_path: Option<PathBuf>,
    pub imported_record_count: usize,
    pub imported_attachment_count: usize,
}

#[derive(Debug, Clone)]
struct ImportBundle {
    manifest: ExportManifestV1,
    payloads: HashMap<String, Vec<u8>>,
}

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("beitang")
}

pub fn default_export_file_name() -> String {
    format!(
        "beitang-export-{}.zip",
        Local::now().format("%Y%m%d-%H%M%S")
    )
}

pub fn export_archive(db: &Database, destination: &Path) -> Result<ExportResult, String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建导出目录失败 {}: {}", parent.display(), err))?;
    }

    let records = db
        .get_all_records()
        .map_err(|err| format!("读取记录失败: {}", err))?;
    let tags = db
        .get_tags()
        .map_err(|err| format!("读取标签失败: {}", err))?;
    let persons = db
        .get_persons()
        .map_err(|err| format!("读取人物失败: {}", err))?;
    let attachments = db
        .get_all_attachments_metadata()
        .map_err(|err| format!("读取附件失败: {}", err))?;

    let file = File::create(destination)
        .map_err(|err| format!("创建导出文件失败 {}: {}", destination.display(), err))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut export_attachments = Vec::with_capacity(attachments.len());
    for attachment in &attachments {
        let should_have_payload = attachment.status == AttachmentStatus::Ready;
        let payload = db
            .get_attachment_bytes(&attachment.id)
            .map_err(|err| format!("读取附件内容失败 {}: {}", attachment.file_name, err))?;

        if should_have_payload {
            let payload = payload.ok_or_else(|| {
                format!(
                    "附件 {} 处于可用状态，但没有可导出的文件内容",
                    attachment.file_name
                )
            })?;
            zip.start_file(attachment_payload_path(&attachment.id), options)
                .map_err(|err| format!("写入附件归档失败 {}: {}", attachment.file_name, err))?;
            zip.write_all(&payload)
                .map_err(|err| format!("写入附件内容失败 {}: {}", attachment.file_name, err))?;
            export_attachments.push(ExportAttachmentEntry::from_attachment(attachment, true));
        } else {
            export_attachments.push(ExportAttachmentEntry::from_attachment(attachment, false));
        }
    }

    let manifest = ExportManifestV1 {
        format_version: EXPORT_FORMAT_VERSION,
        schema_version: db.schema_version(),
        exported_at: Utc::now(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        records: records.iter().map(ExportRecord::from_record).collect(),
        tags,
        persons,
        attachments: export_attachments,
    };

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|err| format!("序列化导出清单失败: {}", err))?;
    zip.start_file(MANIFEST_PATH, options)
        .map_err(|err| format!("写入导出清单失败: {}", err))?;
    zip.write_all(&manifest_bytes)
        .map_err(|err| format!("写入导出清单失败: {}", err))?;
    zip.finish()
        .map_err(|err| format!("完成导出归档失败: {}", err))?;

    Ok(ExportResult {
        destination: destination.to_path_buf(),
        record_count: manifest.records.len(),
        attachment_count: manifest.attachments.len(),
    })
}

pub fn preview_import_archive(db: &Database, archive_path: &Path) -> Result<ImportPreview, String> {
    let bundle = read_import_bundle(archive_path, false)?;
    let conflicts = collect_conflicts(db, &bundle.manifest.records)?;

    let mut ready_attachment_count = 0usize;
    let mut processing_attachment_count = 0usize;
    let mut failed_attachment_count = 0usize;
    for attachment in &bundle.manifest.attachments {
        match attachment.status {
            AttachmentStatus::Ready => ready_attachment_count += 1,
            AttachmentStatus::Processing => processing_attachment_count += 1,
            AttachmentStatus::Failed => failed_attachment_count += 1,
        }
    }

    Ok(ImportPreview {
        archive_path: archive_path.to_path_buf(),
        record_count: bundle.manifest.records.len(),
        tag_count: bundle.manifest.tags.len(),
        person_count: bundle.manifest.persons.len(),
        attachment_count: bundle.manifest.attachments.len(),
        ready_attachment_count,
        processing_attachment_count,
        failed_attachment_count,
        conflicts,
    })
}

pub fn apply_import_archive(
    db: &Database,
    archive_path: &Path,
    mode: ImportMode,
    resolutions: &[ConflictResolution],
    backup_dir: &Path,
) -> Result<ImportResult, String> {
    let bundle = read_import_bundle(archive_path, true)?;
    let backup_path = if mode == ImportMode::ReplaceWithBackup {
        fs::create_dir_all(backup_dir)
            .map_err(|err| format!("创建备份目录失败 {}: {}", backup_dir.display(), err))?;
        let backup_path = backup_dir.join(default_export_file_name());
        export_archive(db, &backup_path)?;
        Some(backup_path)
    } else {
        None
    };

    match mode {
        ImportMode::ReplaceWithBackup => replace_import(db, &bundle)?,
        ImportMode::Merge => merge_import(db, &bundle, resolutions)?,
    }

    Ok(ImportResult {
        backup_path,
        imported_record_count: bundle.manifest.records.len(),
        imported_attachment_count: bundle.manifest.attachments.len(),
    })
}

fn replace_import(db: &Database, bundle: &ImportBundle) -> Result<(), String> {
    db.clear_business_data()
        .map_err(|err| format!("清空现有数据失败: {}", err))?;
    apply_manifest_contents(db, bundle, None)
}

fn merge_import(
    db: &Database,
    bundle: &ImportBundle,
    resolutions: &[ConflictResolution],
) -> Result<(), String> {
    let resolution_map: HashMap<Uuid, ConflictChoice> = resolutions
        .iter()
        .map(|resolution| (resolution.record_id, resolution.choice))
        .collect();
    let conflicts = collect_conflicts(db, &bundle.manifest.records)?;
    for conflict in &conflicts {
        if !resolution_map.contains_key(&conflict.record_id) {
            return Err(format!(
                "记录 {} 尚未选择冲突处理方案",
                conflict.display_title
            ));
        }
    }

    apply_manifest_contents(db, bundle, Some(&resolution_map))
}

fn apply_manifest_contents(
    db: &Database,
    bundle: &ImportBundle,
    resolutions: Option<&HashMap<Uuid, ConflictChoice>>,
) -> Result<(), String> {
    for tag in &bundle.manifest.tags {
        db.upsert_tag_metadata(tag)
            .map_err(|err| format!("写入标签失败 {}: {}", tag.name, err))?;
    }
    for person in &bundle.manifest.persons {
        db.upsert_person_metadata(person)
            .map_err(|err| format!("写入人物失败 {}: {}", person.name, err))?;
    }

    let attachment_map = attachment_map_by_record(&bundle.manifest.attachments);
    for export_record in &bundle.manifest.records {
        let record = export_record.to_record();
        let should_import = match resolutions.and_then(|map| map.get(&record.id)).copied() {
            Some(ConflictChoice::KeepLocal) => false,
            Some(ConflictChoice::UseImported) | None => true,
        };
        if !should_import {
            continue;
        }

        db.create_record(&record)
            .map_err(|err| format!("写入记录失败 {}: {}", record.display_title(), err))?;
        db.delete_attachments_for_record(record.id)
            .map_err(|err| format!("清理附件失败 {}: {}", record.display_title(), err))?;

        if let Some(entries) = attachment_map.get(&record.id.to_string()) {
            for attachment_entry in entries {
                let attachment = attachment_entry.to_attachment();
                let payload = bundle.payloads.get(&attachment.id).map(Vec::as_slice);
                db.create_attachment_with_data(&attachment, payload)
                    .map_err(|err| format!("写入附件失败 {}: {}", attachment.file_name, err))?;
            }
        }
    }

    Ok(())
}

fn collect_conflicts(
    db: &Database,
    records: &[ExportRecord],
) -> Result<Vec<ImportConflict>, String> {
    let mut conflicts = Vec::new();
    for export_record in records {
        if let Some(local_record) = db
            .get_record_by_id(export_record.id)
            .map_err(|err| format!("读取本地记录失败: {}", err))?
        {
            conflicts.push(ImportConflict {
                record_id: export_record.id,
                display_title: export_record.display_title(),
                record_type: export_record.record_type.clone(),
                local_updated_at: local_record.updated_at,
                imported_updated_at: export_record.updated_at,
            });
        }
    }
    Ok(conflicts)
}

fn attachment_map_by_record<'a>(
    attachments: &'a [ExportAttachmentEntry],
) -> HashMap<String, Vec<&'a ExportAttachmentEntry>> {
    let mut map = HashMap::new();
    for attachment in attachments {
        map.entry(attachment.record_id.clone())
            .or_insert_with(Vec::new)
            .push(attachment);
    }
    map
}

fn read_import_bundle(path: &Path, load_payloads: bool) -> Result<ImportBundle, String> {
    let file =
        File::open(path).map_err(|err| format!("打开导入归档失败 {}: {}", path.display(), err))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|err| format!("读取导入归档失败 {}: {}", path.display(), err))?;
    let manifest = read_manifest(&mut archive)?;
    if manifest.format_version != EXPORT_FORMAT_VERSION {
        return Err(format!(
            "不支持的导入格式版本 {}，当前仅支持 {}",
            manifest.format_version, EXPORT_FORMAT_VERSION
        ));
    }

    let mut payloads = HashMap::new();
    for attachment in &manifest.attachments {
        if attachment.has_payload {
            let mut entry = archive
                .by_name(&attachment_payload_path(&attachment.id))
                .map_err(|err| format!("归档中缺少附件内容 {}: {}", attachment.file_name, err))?;
            if load_payloads {
                let mut bytes = Vec::new();
                entry
                    .read_to_end(&mut bytes)
                    .map_err(|err| format!("读取附件内容失败 {}: {}", attachment.file_name, err))?;
                payloads.insert(attachment.id.clone(), bytes);
            }
        } else if attachment.status == AttachmentStatus::Ready {
            return Err(format!(
                "附件 {} 标记为可用，但归档中没有文件内容",
                attachment.file_name
            ));
        }
    }

    Ok(ImportBundle { manifest, payloads })
}

fn read_manifest(archive: &mut ZipArchive<File>) -> Result<ExportManifestV1, String> {
    let mut manifest_file = archive
        .by_name(MANIFEST_PATH)
        .map_err(|err| format!("归档中缺少 {}: {}", MANIFEST_PATH, err))?;
    let mut manifest_json = String::new();
    manifest_file
        .read_to_string(&mut manifest_json)
        .map_err(|err| format!("读取导入清单失败: {}", err))?;
    serde_json::from_str(&manifest_json).map_err(|err| format!("解析导入清单失败: {}", err))
}

fn attachment_payload_path(attachment_id: &str) -> String {
    format!("attachments/{attachment_id}/payload")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AttachmentStatus, Priority};
    use tempfile::TempDir;

    fn setup_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn export_and_preview_round_trip_preserves_attachment_statuses() {
        let (db, temp_dir) = setup_db();
        let mut record = Record::new_task("Task".to_string(), "Body".to_string(), Priority::High);
        record.tags = vec!["work".to_string()];
        db.create_record(&record).unwrap();

        let ready_attachment = Attachment {
            id: Uuid::new_v4().to_string(),
            record_id: record.id.to_string(),
            file_name: "ready.png".to_string(),
            file_path: "db://attachment/ready".to_string(),
            file_size: 4,
            mime_type: "image/png".to_string(),
            width: 1,
            height: 1,
            created_at: Utc::now(),
            status: AttachmentStatus::Ready,
            error_message: None,
            source_path: None,
        };
        db.create_attachment_with_data(&ready_attachment, Some(&[1, 2, 3, 4]))
            .unwrap();

        let failed_attachment = Attachment {
            id: Uuid::new_v4().to_string(),
            record_id: record.id.to_string(),
            file_name: "failed.png".to_string(),
            file_path: "db://attachment/failed".to_string(),
            file_size: 0,
            mime_type: "image/png".to_string(),
            width: 1,
            height: 1,
            created_at: Utc::now(),
            status: AttachmentStatus::Failed,
            error_message: Some("decode failed".to_string()),
            source_path: None,
        };
        db.create_attachment_with_data(&failed_attachment, None)
            .unwrap();

        let export_path = temp_dir.path().join("snapshot.zip");
        export_archive(&db, &export_path).unwrap();
        let preview = preview_import_archive(&db, &export_path).unwrap();

        assert_eq!(preview.record_count, 1);
        assert_eq!(preview.ready_attachment_count, 1);
        assert_eq!(preview.failed_attachment_count, 1);
    }

    #[test]
    fn replace_import_restores_ready_and_failed_attachments() {
        let (source_db, source_temp) = setup_db();
        let record = Record::new_note("Picture note".to_string());
        source_db.create_record(&record).unwrap();

        let ready_attachment = Attachment {
            id: Uuid::new_v4().to_string(),
            record_id: record.id.to_string(),
            file_name: "ready.jpg".to_string(),
            file_path: "db://attachment/ready".to_string(),
            file_size: 3,
            mime_type: "image/jpeg".to_string(),
            width: 2,
            height: 2,
            created_at: Utc::now(),
            status: AttachmentStatus::Ready,
            error_message: None,
            source_path: None,
        };
        source_db
            .create_attachment_with_data(&ready_attachment, Some(&[4, 5, 6]))
            .unwrap();

        let failed_attachment = Attachment {
            id: Uuid::new_v4().to_string(),
            record_id: record.id.to_string(),
            file_name: "failed.jpg".to_string(),
            file_path: "db://attachment/failed".to_string(),
            file_size: 0,
            mime_type: "image/jpeg".to_string(),
            width: 2,
            height: 2,
            created_at: Utc::now(),
            status: AttachmentStatus::Failed,
            error_message: Some("network lost".to_string()),
            source_path: None,
        };
        source_db
            .create_attachment_with_data(&failed_attachment, None)
            .unwrap();

        let archive_path = source_temp.path().join("replace-import.zip");
        export_archive(&source_db, &archive_path).unwrap();

        let (target_db, target_temp) = setup_db();
        apply_import_archive(
            &target_db,
            &archive_path,
            ImportMode::ReplaceWithBackup,
            &[],
            &target_temp.path().join("backups"),
        )
        .unwrap();

        let imported_record = target_db.get_record_by_id(record.id).unwrap().unwrap();
        assert_eq!(imported_record.display_title(), "Picture note");
        let imported_attachments = target_db.get_attachments(record.id).unwrap();
        assert_eq!(imported_attachments.len(), 2);
        assert!(imported_attachments
            .iter()
            .any(|attachment| attachment.status == AttachmentStatus::Ready));
        assert!(imported_attachments
            .iter()
            .any(|attachment| attachment.status == AttachmentStatus::Failed));

        let ready_bytes = target_db
            .get_attachment_bytes(&ready_attachment.id)
            .unwrap()
            .unwrap();
        assert_eq!(ready_bytes, vec![4, 5, 6]);
        assert!(target_db
            .get_attachment_bytes(&failed_attachment.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn merge_import_respects_conflict_resolution() {
        let (source_db, source_temp) = setup_db();
        let record_id = Uuid::new_v4();
        let source_record = Record {
            id: record_id,
            title: Some("Same".to_string()),
            content: "imported".to_string(),
            priority: Some(Priority::High),
            status: Some(crate::models::TaskStatus::Todo),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            scheduled_for: None,
            due_date: None,
            notified_at: None,
            cancelled_reason: None,
            record_type: RecordType::Task,
            tags: vec!["imported".to_string()],
            persons: Vec::new(),
        };
        source_db.create_record(&source_record).unwrap();
        let archive_path = source_temp.path().join("merge-import.zip");
        export_archive(&source_db, &archive_path).unwrap();

        let (target_db, target_temp) = setup_db();
        let mut local_record = source_record.clone();
        local_record.content = "local".to_string();
        local_record.tags = vec!["local".to_string()];
        local_record.updated_at = Utc::now() + chrono::Duration::minutes(1);
        target_db.create_record(&local_record).unwrap();

        apply_import_archive(
            &target_db,
            &archive_path,
            ImportMode::Merge,
            &[ConflictResolution {
                record_id,
                choice: ConflictChoice::KeepLocal,
            }],
            &target_temp.path().join("backups"),
        )
        .unwrap();
        let kept_record = target_db.get_record_by_id(record_id).unwrap().unwrap();
        assert_eq!(kept_record.content, "local");
        assert_eq!(kept_record.tags, vec!["local".to_string()]);

        apply_import_archive(
            &target_db,
            &archive_path,
            ImportMode::Merge,
            &[ConflictResolution {
                record_id,
                choice: ConflictChoice::UseImported,
            }],
            &target_temp.path().join("backups"),
        )
        .unwrap();
        let merged_record = target_db.get_record_by_id(record_id).unwrap().unwrap();
        assert_eq!(merged_record.content, "imported");
        assert_eq!(merged_record.tags, vec!["imported".to_string()]);
    }
}
