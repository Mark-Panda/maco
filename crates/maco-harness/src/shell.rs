//! 会话级 shell 工具：临时目录指向 `~/.maco/tmp`，工作目录指向会话工作区（含 Git worktree）。

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use adk_core::{AdkError, Result, Tool, ToolContext};
use async_trait::async_trait;
use maco_core::{bash_command_targets_main_repo, SessionWorkspace};
use serde::Deserialize;
use serde_json::{json, Value};

/// 执行 shell 命令；工作目录为会话工作区（worktree 或项目根）。
pub struct MacoBashTool {
    scratch_dir: PathBuf,
    workspace: Option<SessionWorkspace>,
}

impl MacoBashTool {
    pub fn new(scratch_dir: PathBuf, workspace: Option<SessionWorkspace>) -> Self {
        Self {
            scratch_dir,
            workspace,
        }
    }
}

#[derive(Debug, Deserialize)]
struct BashArgs {
    command: String,
}

#[async_trait]
impl Tool for MacoBashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a shell command. TMPDIR points to the session scratch dir; cwd is the session \
         workspace (Git worktree when enabled). Use relative paths from the workspace root."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to run" }
            },
            "required": ["command"]
        }))
    }

    async fn execute(&self, _ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let args: BashArgs = serde_json::from_value(args)
            .map_err(|e| AdkError::tool(format!("invalid bash arguments: {e}")))?;
        if let Some(ref ws) = self.workspace {
            if ws.uses_worktree {
                if let Some(reason) =
                    bash_command_targets_main_repo(&args.command, &ws.repo_root, &ws.workspace_root)
                {
                    return Err(AdkError::tool(format!(
                        "command blocked ({reason}); edit files in the worktree workspace only"
                    )));
                }
            }
        }
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-lc")
            .arg(&args.command)
            .stdin(Stdio::null())
            .env("TMPDIR", &self.scratch_dir)
            .env("TEMP", &self.scratch_dir)
            .env("TMP", &self.scratch_dir)
            .env("MACO_SCRATCH_DIR", &self.scratch_dir);
        if let Some(ref ws) = self.workspace {
            cmd.current_dir(&ws.workspace_root)
                .env("MACO_WORKSPACE_ROOT", &ws.workspace_root)
                .env("MACO_PROJECT_ROOT", &ws.repo_root)
                .env("MACO_GIT_REPO_ROOT", &ws.repo_root);
            if ws.uses_worktree {
                cmd.env("MACO_GIT_WORKTREE", "1");
                if let Some(ref branch) = ws.worktree_branch {
                    cmd.env("MACO_GIT_BRANCH", branch);
                }
            }
        }
        let output = cmd
            .output()
            .await
            .map_err(|e| AdkError::tool(format!("failed to execute bash command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);
        let (workspace, repo, branch) = self
            .workspace
            .as_ref()
            .map(|ws| {
                (
                    ws.workspace_root.display().to_string(),
                    ws.repo_root.display().to_string(),
                    ws.worktree_branch.clone().unwrap_or_else(|| "-".into()),
                )
            })
            .unwrap_or_else(|| ("(none)".into(), "(none)".into(), "-".into()));
        Ok(Value::String(format!(
            "workspace_root: {workspace}\nrepo_root: {repo}\nworktree_branch: {branch}\nscratch_dir: {}\n{stdout}{stderr}\nexit_code: {exit_code}\n",
            self.scratch_dir.display()
        )))
    }
}
