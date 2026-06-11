//! Agent 工具执行后捕获写入的文件并登记为会话附件。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use adk_core::AfterToolCallback;
use maco_core::{MacoResult, SseEnvelope};
use maco_storage::ArtifactStore;
use maco_telemetry::MacoCallbackLogger;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use maco_governance::prepare_log_payload;

use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;

const PREVIEW_TEXT_LIMIT: usize = 512 * 1024;

/// 单次 Run 内 scratch 目录已见文件集合，用于 bash 后 diff 新文件。
pub fn snapshot_scratch_files(scratch_dir: &Path) -> HashSet<PathBuf> {
    let mut known = HashSet::new();
    if scratch_dir.is_dir() {
        collect_files(scratch_dir, scratch_dir, &mut known);
    }
    known
}

fn collect_files(base: &Path, dir: &Path, out: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out);
        } else if path.is_file() {
            out.insert(path);
        }
    }
}

fn diff_new_scratch_files(scratch_dir: &Path, known: &mut HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut fresh = Vec::new();
    if !scratch_dir.is_dir() {
        return fresh;
    }
    let mut current = HashSet::new();
    collect_files(scratch_dir, scratch_dir, &mut current);
    for path in current {
        if known.insert(path.clone()) {
            fresh.push(path);
        }
    }
    fresh
}

fn extract_write_paths(tool_name: &str, args: &Value) -> Vec<PathBuf> {
    let lower = tool_name.to_lowercase();
    let looks_like_write = lower.contains("write")
        || lower.contains("edit")
        || lower.contains("create")
        || lower.ends_with("_save");
    if !looks_like_write {
        return Vec::new();
    }
    let mut paths = Vec::new();
    for key in ["path", "file_path", "filepath", "target", "destination", "file"] {
        if let Some(s) = args.get(key).and_then(|v| v.as_str()) {
            paths.push(PathBuf::from(s));
        }
    }
    paths
}

fn path_allowed(path: &Path, scratch_dir: &Path, project_root: Option<&Path>) -> bool {
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    if let Ok(scratch) = scratch_dir.canonicalize() {
        if canonical.starts_with(&scratch) {
            return true;
        }
    }
    if let Some(root) = project_root {
        if let Ok(project) = root.canonicalize() {
            return canonical.starts_with(&project);
        }
    }
    false
}

pub struct ArtifactCaptureState {
    pub session_id: String,
    pub run_id: String,
    pub artifacts: Arc<ArtifactStore>,
    pub scratch_dir: PathBuf,
    pub project_root: Option<PathBuf>,
    pub scratch_known: Arc<Mutex<HashSet<PathBuf>>>,
    pub sse_tx: mpsc::Sender<SseEnvelope>,
    pub streams: RunStreamRegistry,
    pub orchestrator: RunOrchestrator,
}

async fn publish_artifact_created(state: &ArtifactCaptureState, record: &maco_db::ArtifactRecord) {
    if let Ok(seq) = state.orchestrator.next_seq(&state.run_id).await {
        let env = SseEnvelope {
            event_type: "artifact_created".into(),
            run_id: state.run_id.clone(),
            seq,
            payload: serde_json::json!({
                "id": record.id,
                "filename": record.filename,
                "mime_type": record.mime_type,
                "size_bytes": record.size_bytes,
            }),
        };
        let _ = state.sse_tx.send(env.clone()).await;
        state.streams.publish(&state.session_id, env).await;
    }
}

async fn try_import_paths(
    state: &ArtifactCaptureState,
    paths: Vec<PathBuf>,
) -> MacoResult<()> {
    for path in paths {
        if !path_allowed(&path, &state.scratch_dir, state.project_root.as_deref()) {
            continue;
        }
        if let Some(record) = state
            .artifacts
            .import_from_path(&state.session_id, &path)
            .await?
        {
            publish_artifact_created(state, &record).await;
        }
    }
    Ok(())
}

/// `after_tool`：写日志 + 捕获 Agent 产出的文件。
pub fn after_tool_with_artifacts(
    logger: Arc<MacoCallbackLogger>,
    capture: Arc<ArtifactCaptureState>,
) -> AfterToolCallback {
    Box::new(move |ctx| {
        let logger = Arc::clone(&logger);
        let capture = Arc::clone(&capture);
        Box::pin(async move {
            let tool_name = ctx.tool_name().unwrap_or("unknown");
            let args = ctx.tool_input().cloned().unwrap_or(Value::Null);
            let outcome = ctx.tool_outcome();
            let error_message = outcome.as_ref().and_then(|o| o.error_message.clone());
            let output = outcome
                .as_ref()
                .map(|o| {
                    prepare_log_payload(&serde_json::json!({
                        "success": o.success,
                        "duration_ms": o.duration.as_millis(),
                    }))
                })
                .unwrap_or_else(|| "{}".into());
            logger
                .log_tool_end(tool_name, &output, error_message.as_deref())
                .await;

            let success = outcome.as_ref().map(|o| o.success).unwrap_or(false);
            if success {
                let mut paths = extract_write_paths(tool_name, &args);
                if tool_name == "bash" {
                    let mut known = capture.scratch_known.lock().await;
                    paths.extend(diff_new_scratch_files(&capture.scratch_dir, &mut known));
                }
                let _ = try_import_paths(&capture, paths).await;
            }

            Ok(None)
        })
    })
}

/// 文本预览截断上限（与 API 一致）。
pub fn preview_text_limit() -> usize {
    PREVIEW_TEXT_LIMIT
}
