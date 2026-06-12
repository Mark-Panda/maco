//! 解析会话 Agent 实际工作目录（Git worktree 或项目根）。

use std::path::PathBuf;

use crate::error::MacoResult;
use crate::git_worktree::{ensure_worktree, is_git_repository, DEFAULT_GIT_BRANCH_PREFIX};
use crate::resolve_project_root;

/// 会话工作区解析结果。
#[derive(Debug, Clone)]
pub struct SessionWorkspace {
    /// Git 仓库根（用户绑定的 project_root）。
    pub repo_root: PathBuf,
    /// Agent 应编辑的目录。
    pub workspace_root: PathBuf,
    /// 是否通过 worktree 隔离。
    pub uses_worktree: bool,
    /// worktree 分支（仅 `uses_worktree` 时有值）。
    pub worktree_branch: Option<String>,
}

/// 根据会话元数据解析/创建 Agent 工作区。
pub fn resolve_session_workspace(
    project_root: Option<&str>,
    session_id: &str,
    git_worktree_enabled: bool,
    git_branch_prefix: &str,
) -> MacoResult<Option<SessionWorkspace>> {
    let Some(repo) = resolve_project_root(project_root)? else {
        return Ok(None);
    };
    if !git_worktree_enabled || !is_git_repository(&repo) {
        return Ok(Some(SessionWorkspace {
            repo_root: repo.clone(),
            workspace_root: repo,
            uses_worktree: false,
            worktree_branch: None,
        }));
    }
    let prefix = if git_branch_prefix.trim().is_empty() {
        DEFAULT_GIT_BRANCH_PREFIX
    } else {
        git_branch_prefix.trim()
    };
    let Some(info) = ensure_worktree(&repo, session_id, prefix)? else {
        return Ok(Some(SessionWorkspace {
            repo_root: repo.clone(),
            workspace_root: repo,
            uses_worktree: false,
            worktree_branch: None,
        }));
    };
    Ok(Some(SessionWorkspace {
        repo_root: info.repo_root,
        workspace_root: info.worktree_path,
        uses_worktree: true,
        worktree_branch: Some(info.branch),
    }))
}

/// 从已缓存的元数据快速解析工作区（不触发 git）。
pub fn workspace_from_cached(
    project_root: Option<&str>,
    git_worktree_enabled: bool,
    git_worktree_path: Option<&str>,
    git_worktree_branch: Option<&str>,
) -> MacoResult<Option<SessionWorkspace>> {
    let Some(repo) = resolve_project_root(project_root)? else {
        return Ok(None);
    };
    if git_worktree_enabled {
        if let Some(path) = git_worktree_path.filter(|p| !p.trim().is_empty()) {
            let workspace = PathBuf::from(path);
            if workspace.exists() {
                return Ok(Some(SessionWorkspace {
                    repo_root: repo.clone(),
                    workspace_root: workspace,
                    uses_worktree: true,
                    worktree_branch: git_worktree_branch.map(str::to_string),
                }));
            }
        }
    }
    Ok(Some(SessionWorkspace {
        repo_root: repo.clone(),
        workspace_root: repo,
        uses_worktree: false,
        worktree_branch: None,
    }))
}
