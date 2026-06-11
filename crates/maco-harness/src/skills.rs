//! 从 `~/.maco/skills/` 扫描 Markdown Skill 文件，注入 Agent 系统指令。

use std::path::{Path, PathBuf};

use maco_core::default_skills_dir;
use serde::{Deserialize, Serialize};

/// 单个 Skill 的定义（文件名 + 正文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    /// Skill 名称（取自文件名）。
    pub name: String,
    /// 简短描述。
    pub description: String,
    /// SKILL.md 正文（注入系统提示）。
    pub content: String,
    /// 源文件路径。
    pub file_path: PathBuf,
}

/// 扫描本地 Skill 目录（默认 `~/.maco/skills/**/*.md`）。
pub struct SkillLoader;

impl SkillLoader {
    /// 递归读取目录下所有 `.md` 文件；`dir` 为 `None` 时使用默认路径。
    pub fn scan(dir: Option<&Path>) -> Vec<SkillDefinition> {
        let root = dir.map(PathBuf::from).unwrap_or_else(default_skills_dir);
        let mut skills = Vec::new();
        let pattern = root.join("**/*.md");
        let pattern_str = pattern.to_string_lossy().to_string();
        for entry in glob::glob(&pattern_str).into_iter().flatten().flatten() {
            if let Ok(content) = std::fs::read_to_string(&entry) {
                let name = entry
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("skill")
                    .to_string();
                skills.push(SkillDefinition {
                    name: name.clone(),
                    description: format!("Skill from {}", entry.display()),
                    content,
                    file_path: entry,
                });
            }
        }
        skills
    }
}
