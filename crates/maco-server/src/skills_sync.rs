//! 将磁盘 Skill 与 `maco_skills` 表、ADK `SkillIndex` 同步。

use maco_core::MacoResult;
use maco_db::SkillRepo;
use maco_harness::{AdkSkillManager, SkillDocument};
use serde::Serialize;

/// Skill 列表项（`GET /skills`）。
#[derive(Debug, Clone, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub enabled: bool,
}

/// `POST /skills` 上传 zip 的响应。
#[derive(Debug, Clone, Serialize)]
pub struct SkillUploadResponse {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub extracted_files: usize,
    pub skill: SkillSummary,
}

pub async fn sync_skills(
    skills: &SkillRepo,
    manager: &AdkSkillManager,
    project_root: Option<&std::path::Path>,
) -> MacoResult<Vec<SkillSummary>> {
    manager
        .reload_from_disk(project_root)
        .map_err(|e| maco_core::MacoError::config(e.to_string()))?;

    let index = manager.full_index();
    let names: Vec<String> = index.skills().iter().map(|s| s.name.clone()).collect();

    for doc in index.skills() {
        skills
            .upsert_from_scan(
                &doc.name,
                &doc.description,
                &doc.path.display().to_string(),
            )
            .await?;
    }
    skills.delete_not_in(&names).await?;

    reload_disabled(skills, manager).await?;
    Ok(summaries_from_index(&index, manager))
}

pub async fn reload_disabled(skills: &SkillRepo, manager: &AdkSkillManager) -> MacoResult<()> {
    let records = skills.list().await?;
    let disabled: Vec<String> = records
        .iter()
        .filter(|r| r.enabled == 0)
        .map(|r| r.name.clone())
        .collect();
    manager.set_disabled_names(disabled);
    Ok(())
}

pub fn summaries_from_index(index: &maco_harness::SkillIndex, manager: &AdkSkillManager) -> Vec<SkillSummary> {
    index
        .skills()
        .iter()
        .map(|doc| doc_to_summary(doc, manager.is_enabled(&doc.name)))
        .collect()
}

pub fn doc_to_summary(doc: &SkillDocument, enabled: bool) -> SkillSummary {
    let updated_at = doc.last_modified.and_then(|secs| {
        chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
    });
    SkillSummary {
        name: doc.name.clone(),
        description: doc.description.clone(),
        file_path: doc.path.display().to_string(),
        updated_at,
        enabled,
    }
}

pub fn skill_to_summary(doc: &SkillDocument, enabled: bool) -> SkillSummary {
    doc_to_summary(doc, enabled)
}
