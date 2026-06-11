//! 会话附件：磁盘存储 + `maco_artifacts` 元数据。

use std::path::{Path, PathBuf};

use maco_core::{MacoError, MacoResult};
use maco_db::{ArtifactRecord, ArtifactRepo};
use maco_governance::{mime_for_artifact, validate_artifact};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// 按 `session_id/artifact_id` 组织文件，并与 DB 记录同步。
pub struct ArtifactStore {
    base_dir: PathBuf,
    repo: ArtifactRepo,
}

impl ArtifactStore {
    pub fn new(base_dir: PathBuf, repo: ArtifactRepo) -> Self {
        Self { base_dir, repo }
    }

    pub fn storage_path(&self, session_id: &str, artifact_id: &str) -> PathBuf {
        self.base_dir.join(session_id).join(artifact_id)
    }

    pub async fn save(
        &self,
        session_id: &str,
        filename: &str,
        mime_type: &str,
        bytes: &[u8],
    ) -> MacoResult<ArtifactRecord> {
        validate_artifact(mime_type, bytes.len())
            .map_err(|e| MacoError::config(e.to_string()))?;

        let id = Uuid::new_v4().to_string();
        let session_dir = self.base_dir.join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| MacoError::config(format!("create artifact dir: {e}")))?;

        let path = session_dir.join(&id);
        std::fs::write(&path, bytes)
            .map_err(|e| MacoError::config(format!("write artifact: {e}")))?;

        let checksum = hex::encode(Sha256::digest(bytes));
        self.repo
            .insert(
                session_id,
                filename,
                mime_type,
                bytes.len() as i64,
                &path.display().to_string(),
                Some(&checksum),
            )
            .await
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// 附件元数据仓库（列表查询等）。
    pub fn repo(&self) -> &ArtifactRepo {
        &self.repo
    }

    /// 从磁盘路径导入为会话附件（Agent 写文件等）；同 checksum 已存在则跳过。
    pub async fn import_from_path(
        &self,
        session_id: &str,
        source_path: &Path,
    ) -> MacoResult<Option<ArtifactRecord>> {
        if !source_path.is_file() {
            return Ok(None);
        }
        let bytes = std::fs::read(source_path)
            .map_err(|e| MacoError::config(format!("read source file: {e}")))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let checksum = hex::encode(Sha256::digest(&bytes));
        let existing = self.repo.list_for_session(session_id).await?;
        if existing.iter().any(|r| r.checksum.as_deref() == Some(checksum.as_str())) {
            return Ok(None);
        }
        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let mime_type = mime_for_artifact(&filename, &bytes);
        let record = self
            .save(session_id, &filename, &mime_type, &bytes)
            .await?;
        Ok(Some(record))
    }

    /// 读取会话附件二进制（校验 session 归属）。
    pub async fn read(
        &self,
        session_id: &str,
        artifact_id: &str,
    ) -> MacoResult<(ArtifactRecord, Vec<u8>)> {
        let record = self
            .repo
            .get(artifact_id)
            .await?
            .ok_or_else(|| MacoError::not_found("artifact"))?;
        if record.session_id != session_id {
            return Err(MacoError::not_found("artifact"));
        }
        let bytes = std::fs::read(&record.storage_path)
            .map_err(|e| MacoError::config(format!("read artifact: {e}")))?;
        Ok((record, bytes))
    }
}
