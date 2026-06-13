//! Agent 工具执行后捕获写入的文件并登记为会话附件。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use adk_core::AfterToolCallback;
use maco_core::{MacoResult, SseEnvelope};
use maco_storage::ArtifactStore;
use maco_telemetry::MacoCallbackLogger;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use maco_governance::prepare_log_payload;

use crate::orchestrator::RunOrchestrator;
use crate::run_stream::RunStreamRegistry;

const PREVIEW_TEXT_LIMIT: usize = 512 * 1024;

/// 单次 Run 内 scratch 目录已见文件集合，用于 bash 后 diff 新文件。
pub fn snapshot_scratch_files(scratch_dir: &Path) -> HashSet<PathBuf> {
    let mut known = HashSet::new();
    if scratch_dir.is_dir() {
        collect_files(scratch_dir, &mut known);
    }
    known
}

fn collect_files(dir: &Path, out: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
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
    collect_files(scratch_dir, &mut current);
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
    for key in [
        "path",
        "file_path",
        "filepath",
        "target",
        "destination",
        "file",
    ] {
        if let Some(s) = args.get(key).and_then(|v| v.as_str()) {
            paths.push(expand_tilde(s));
        }
    }
    paths
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn extract_bash_output_paths(command: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for segment in command.split(['|', ';', '&']) {
        let segment = segment.trim();
        if let Some(idx) = segment.rfind(">>") {
            if let Some(token) = segment[idx + 2..].split_whitespace().next() {
                let token = token.trim_matches('"').trim_matches('\'');
                if token != "/dev/null" {
                    paths.push(expand_tilde(token));
                }
            }
        } else if let Some(idx) = segment.rfind('>') {
            if segment.as_bytes().get(idx.saturating_sub(1)) == Some(&b'=') {
                continue;
            }
            if let Some(token) = segment[idx + 1..].split_whitespace().next() {
                let token = token.trim_matches('"').trim_matches('\'');
                if token != "/dev/null" {
                    paths.push(expand_tilde(token));
                }
            }
        }
    }
    for token in command.split_whitespace() {
        let token = token.trim_matches('"').trim_matches('\'');
        if (token.starts_with('/') || token.starts_with("~/"))
            && token.contains('.')
            && !token.starts_with("-")
        {
            paths.push(expand_tilde(token));
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn path_importable(
    path: &Path,
    scratch_dir: &Path,
    project_root: Option<&Path>,
    explicit_paths: &[PathBuf],
) -> bool {
    if path_allowed(path, scratch_dir, project_root) {
        return true;
    }
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    explicit_paths.iter().any(|explicit| {
        explicit
            .canonicalize()
            .ok()
            .map(|c| c == canonical)
            .unwrap_or(false)
    })
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

fn path_allowed(path: &Path, scratch_dir: &Path, project_root: Option<&Path>) -> bool {
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    if let Ok(scratch) = scratch_dir.canonicalize()
        && canonical.starts_with(&scratch)
    {
        return true;
    }
    if let Some(root) = project_root
        && let Ok(project) = root.canonicalize()
    {
        return canonical.starts_with(&project);
    }
    false
}

async fn publish_tasks_updated(state: &ArtifactCaptureState) {
    if let Ok(seq) = state.orchestrator.next_seq(&state.run_id).await {
        let env = SseEnvelope {
            event_type: "tasks_updated".into(),
            run_id: state.run_id.clone(),
            seq,
            payload: serde_json::json!({}),
        };
        let _ = state.sse_tx.send(env.clone()).await;
        state.streams.publish(&state.session_id, env).await;
    }
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
    explicit_paths: &[PathBuf],
) -> MacoResult<()> {
    for path in paths {
        if !path_importable(
            &path,
            &state.scratch_dir,
            state.project_root.as_deref(),
            explicit_paths,
        ) {
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
                let mut explicit_paths = extract_write_paths(tool_name, &args);
                let mut paths = explicit_paths.clone();
                if tool_name == "bash" {
                    if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
                        explicit_paths.extend(extract_bash_output_paths(command));
                        paths.extend(extract_bash_output_paths(command));
                    }
                    let mut known = capture.scratch_known.lock().await;
                    paths.extend(diff_new_scratch_files(&capture.scratch_dir, &mut known));
                }
                explicit_paths.sort();
                explicit_paths.dedup();
                let _ = try_import_paths(&capture, paths, &explicit_paths).await;
                if tool_name == "update_plan" || tool_name == "upsert_todo" {
                    publish_tasks_updated(&capture).await;
                }
            }

            Ok(None)
        })
    })
}

/// 文本预览截断上限（与 API 一致）。
pub fn preview_text_limit() -> usize {
    PREVIEW_TEXT_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_bash_output_paths_finds_redirect_and_tilde() {
        let paths = extract_bash_output_paths(
            "mkdir -p ~/Desktop/gomoku && cat > ~/Desktop/gomoku/index.html <<'EOF'",
        );
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with("Desktop/gomoku/index.html"))
        );
    }
}
