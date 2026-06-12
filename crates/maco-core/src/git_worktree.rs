//! Git worktree：为 Agent 会话隔离代码编辑工作区。

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{MacoError, MacoResult};
use crate::expand_tilde_path;
use crate::maco_home_dir;
use crate::resolve_project_root;

/// 默认分支前缀。
pub const DEFAULT_GIT_BRANCH_PREFIX: &str = "maco/agent";

/// 已就绪的 worktree 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreeInfo {
    /// 原始 Git 仓库根目录。
    pub repo_root: PathBuf,
    /// Agent 实际编辑目录（worktree 检出路径）。
    pub worktree_path: PathBuf,
    /// 关联分支名。
    pub branch: String,
}

/// Git 仓库探测结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitRepoProbe {
    /// 路径是 Git 仓库且 git 可用。
    Available,
    /// git 可用，但路径不是仓库。
    NotRepository,
    /// 无法执行 git（未安装或不在 PATH）。
    GitUnavailable,
}

/// 探测路径是否为 Git 仓库，并区分 git 不可用的情况。
pub fn probe_git_repository(path: &Path) -> GitRepoProbe {
    match Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--git-dir"])
        .output()
    {
        Err(e) if e.kind() == io::ErrorKind::NotFound => GitRepoProbe::GitUnavailable,
        Err(_) => GitRepoProbe::GitUnavailable,
        Ok(o) if o.status.success() => GitRepoProbe::Available,
        Ok(_) => GitRepoProbe::NotRepository,
    }
}

/// 判断路径是否为 Git 仓库。
pub fn is_git_repository(path: &Path) -> bool {
    probe_git_repository(path) == GitRepoProbe::Available
}

/// 根据会话元数据计算 worktree 状态（供 API / UI 展示）。
pub fn git_worktree_status(
    enabled: bool,
    project_root: Option<&str>,
    worktree_path: Option<&str>,
) -> &'static str {
    if !enabled {
        return "disabled";
    }
    if project_root.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        return "no_project";
    }
    if worktree_path.map(str::trim).filter(|s| !s.is_empty()).is_some() {
        return "active";
    }
    let Ok(Some(repo)) = resolve_project_root(project_root) else {
        return "no_project";
    };
    match probe_git_repository(&repo) {
        GitRepoProbe::GitUnavailable => "git_unavailable",
        GitRepoProbe::NotRepository => "not_git_repo",
        GitRepoProbe::Available => "pending",
    }
}

/// worktree 模式下检测 bash 命令是否试图操作主仓库（返回阻止原因）。
pub fn bash_command_targets_main_repo(
    command: &str,
    repo_root: &Path,
    workspace_root: &Path,
) -> Option<&'static str> {
    let repo_canon = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let ws_canon = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    if path_literal_in_command(command, &repo_canon) {
        return Some("command contains main repository path");
    }

    for var in ["MACO_GIT_REPO_ROOT", "MACO_PROJECT_ROOT"] {
        if command.contains(var) {
            return Some("command references main repository environment variable");
        }
    }

    for token in tokenize_path_candidates(command) {
        if token_targets_forbidden_repo(&token, &repo_canon, &ws_canon, workspace_root).is_some() {
            return Some("command targets files in the main repository outside the worktree");
        }
    }

    None
}

fn path_literal_in_command(command: &str, path: &Path) -> bool {
    let s = path.to_string_lossy();
    if command.contains(s.as_ref()) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        let lower_cmd = command.to_lowercase();
        let lower_path = s.to_lowercase();
        if lower_cmd.contains(lower_path.as_str()) {
            return true;
        }
    }
    false
}

fn tokenize_path_candidates(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| "\"'`;,|&(){}<>".contains(c)).to_string())
        .filter(|t| {
            !t.is_empty()
                && (t.starts_with('/')
                    || t.starts_with('~')
                    || t.starts_with('.')
                    || t.contains('/'))
        })
        .collect()
}

fn token_targets_forbidden_repo(
    token: &str,
    repo_canon: &Path,
    ws_canon: &Path,
    workspace_root: &Path,
) -> Option<()> {
    let candidate = if token.starts_with('/') || token.starts_with('~') {
        expand_tilde_path(PathBuf::from(token))
    } else if token.starts_with('.') || token.contains('/') {
        workspace_root.join(token)
    } else {
        return None;
    };

    let canon = candidate.canonicalize().ok()?;
    if canon.starts_with(repo_canon) && !canon.starts_with(ws_canon) {
        Some(())
    } else {
        None
    }
}

/// 会话 worktree 目录：`~/.maco/worktrees/{session_id}`。
pub fn worktree_path_for_session(session_id: &str) -> PathBuf {
    maco_home_dir().join("worktrees").join(session_id)
}

/// 根据前缀与会话 ID 生成分支名。
pub fn branch_name(prefix: &str, session_id: &str) -> String {
    let prefix = sanitize_git_ref(prefix.trim().trim_end_matches('/'));
    let short_id: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let short_id = if short_id.is_empty() {
        "session".into()
    } else {
        short_id
    };
    if prefix.is_empty() {
        format!("maco/{short_id}")
    } else {
        format!("{prefix}/{short_id}")
    }
}

fn sanitize_git_ref(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.');
        if ok {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    out.trim_matches(&['-', '/', '.'][..]).to_string()
}

fn git_output(repo_root: &Path, args: &[&str]) -> MacoResult<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|e| MacoError::config(format!("git {}: {e}", args.join(" "))))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(MacoError::config(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )))
    }
}

fn branch_exists(repo_root: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 读取 worktree 当前分支名。
pub fn current_branch(worktree_path: &Path) -> Option<String> {
    Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD")
}

fn worktree_registered(repo_root: &Path, worktree_path: &Path) -> bool {
    let Ok(list) = git_output(repo_root, &["worktree", "list", "--porcelain"]) else {
        return false;
    };
    let target = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    for line in list.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            let p = PathBuf::from(path.trim());
            let canon = p.canonicalize().unwrap_or(p);
            if canon == target {
                return true;
            }
        }
    }
    false
}

fn git_worktree_add(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
    create_branch: bool,
) -> MacoResult<()> {
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MacoError::config(format!("create worktree parent: {e}")))?;
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_root).arg("worktree").arg("add");
    if create_branch {
        cmd.arg("-b").arg(branch);
    }
    cmd.arg(worktree_path).arg(branch);
    let output = cmd
        .output()
        .map_err(|e| MacoError::config(format!("git worktree add: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(MacoError::config(format!("git worktree add failed: {stderr}")))
}

/// 为会话创建或复用 worktree；非 Git 仓库时返回 `None`。
pub fn ensure_worktree(
    repo_root: &Path,
    session_id: &str,
    branch_prefix: &str,
) -> MacoResult<Option<GitWorktreeInfo>> {
    if !is_git_repository(repo_root) {
        return Ok(None);
    }
    let worktree_path = worktree_path_for_session(session_id);
    let branch = branch_name(branch_prefix, session_id);

    if worktree_registered(repo_root, &worktree_path) {
        let actual = current_branch(&worktree_path).unwrap_or_default();
        if actual == branch {
            return Ok(Some(GitWorktreeInfo {
                repo_root: repo_root.to_path_buf(),
                worktree_path,
                branch,
            }));
        }
        tracing::info!(
            "worktree branch mismatch (actual={actual}, expected={branch}), recreating"
        );
        remove_worktree(repo_root, session_id)?;
    } else if worktree_path.exists() {
        remove_worktree(repo_root, session_id)?;
    }

    if branch_exists(repo_root, &branch) {
        git_worktree_add(repo_root, &worktree_path, &branch, false)?;
    } else {
        if let Err(e) = git_worktree_add(repo_root, &worktree_path, &branch, true) {
            if branch_exists(repo_root, &branch) {
                git_worktree_add(repo_root, &worktree_path, &branch, false)?;
            } else {
                return Err(e);
            }
        }
    }

    Ok(Some(GitWorktreeInfo {
        repo_root: repo_root.to_path_buf(),
        worktree_path,
        branch,
    }))
}

/// 移除会话 worktree（忽略不存在的情况）；尽力删除关联本地分支。
pub fn remove_worktree(repo_root: &Path, session_id: &str) -> MacoResult<()> {
    let worktree_path = worktree_path_for_session(session_id);
    let branch = current_branch(&worktree_path);
    if worktree_registered(repo_root, &worktree_path) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["worktree", "remove", "--force"])
            .arg(&worktree_path)
            .output()
            .map_err(|e| MacoError::config(format!("git worktree remove: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("git worktree remove: {stderr}");
        }
    } else if worktree_path.exists() {
        let _ = std::fs::remove_dir_all(&worktree_path);
    }
    if let Some(ref b) = branch {
        let _ = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["branch", "-D", b])
            .status();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_name_uses_prefix_and_session() {
        assert_eq!(
            branch_name("maco/agent", "abc-def-123"),
            "maco/agent/abcdef123"
        );
    }

    #[test]
    fn branch_name_sanitizes_prefix() {
        assert_eq!(
            branch_name("feature/my agent", "sess1"),
            "feature/my-agent/sess1"
        );
    }

    #[test]
    fn bash_guard_blocks_repo_env_var() {
        let repo = PathBuf::from("/tmp/maco-test-repo");
        let wt = PathBuf::from("/tmp/maco-test-wt");
        let cmd = "cd $MACO_GIT_REPO_ROOT && rm -rf .";
        assert!(bash_command_targets_main_repo(cmd, &repo, &wt).is_some());
    }

    #[test]
    fn bash_guard_blocks_absolute_repo_path() {
        let repo = std::env::temp_dir().join("maco-guard-repo");
        let wt = std::env::temp_dir().join("maco-guard-wt");
        let cmd = format!("cat {}/README.md", repo.display());
        assert!(bash_command_targets_main_repo(&cmd, &repo, &wt).is_some());
    }
}
