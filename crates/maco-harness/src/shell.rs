//! 会话级 shell 工具：临时目录指向 `~/.maco/tmp`，工作目录不锁定（可 `cd` 到用户项目）。

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use adk_core::{AdkError, Result, Tool, ToolContext};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

/// 执行 shell 命令；`TMPDIR` 等指向会话临时目录，但不限制 `cd` / 绝对路径。
pub struct MacoBashTool {
    scratch_dir: PathBuf,
    project_root: Option<PathBuf>,
}

impl MacoBashTool {
    pub fn new(scratch_dir: PathBuf, project_root: Option<PathBuf>) -> Self {
        Self {
            scratch_dir,
            project_root,
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
        "Execute a shell command. TMPDIR points to the session scratch dir (~/.maco/tmp/sessions/<id>); \
         use absolute paths or cd when editing a user project."
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
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-lc")
            .arg(&args.command)
            .stdin(Stdio::null())
            .env("TMPDIR", &self.scratch_dir)
            .env("TEMP", &self.scratch_dir)
            .env("TMP", &self.scratch_dir)
            .env("MACO_SCRATCH_DIR", &self.scratch_dir);
        if let Some(ref root) = self.project_root {
            cmd.current_dir(root).env("MACO_PROJECT_ROOT", root);
        }
        let output = cmd
            .output()
            .await
            .map_err(|e| AdkError::tool(format!("failed to execute bash command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);
        let project = self
            .project_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into());
        Ok(Value::String(format!(
            "project_root: {project}\nscratch_dir: {}\n{stdout}{stderr}\nexit_code: {exit_code}\n",
            self.scratch_dir.display()
        )))
    }
}
