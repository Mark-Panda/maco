use std::path::{Path, PathBuf};

use maco_core::{MacoError, MacoResult};
use maco_db::{ArtifactRecord, ArtifactRepo};
use maco_governance::validate_artifact;
use sha2::{Digest, Sha256};
use uuid::Uuid;

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
}
