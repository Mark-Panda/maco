use std::path::{Path, PathBuf};

use maco_core::default_skills_dir;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: PathBuf,
}

pub struct SkillLoader;

impl SkillLoader {
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
