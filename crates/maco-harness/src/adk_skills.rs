//! ADK 原生 Skill 索引：基于 `adk-skill` 发现、选择与注入。

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, RwLock};

use adk_skill::{
    SelectionPolicy, SkillDocument, SkillIndex, SkillResult, load_skill_index_with_extras,
};
use maco_core::default_skills_dir;

/// 管理 ADK `SkillIndex` 与 maco 侧启用/禁用过滤。
#[derive(Clone, Default)]
pub struct AdkSkillManager {
    full_index: Arc<RwLock<SkillIndex>>,
    disabled: Arc<RwLock<HashSet<String>>>,
}

impl AdkSkillManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从项目根 + `~/.maco/skills` 重新加载索引（不过滤禁用项）。
    pub fn reload_from_disk(&self, project_root: Option<&Path>) -> SkillResult<()> {
        let root = project_root
            .filter(|p| p.is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(default_skills_dir);
        let extras = vec![default_skills_dir()];
        let index = load_skill_index_with_extras(&root, &extras)?;
        *self.full_index.write().expect("skill index lock") = index;
        Ok(())
    }

    pub fn set_disabled_names(&self, names: impl IntoIterator<Item = String>) {
        *self.disabled.write().expect("skill disabled lock") =
            names.into_iter().collect::<HashSet<_>>();
    }

    pub fn set_enabled(&self, name: &str, enabled: bool) {
        let mut disabled = self.disabled.write().expect("skill disabled lock");
        if enabled {
            disabled.remove(name);
        } else {
            disabled.insert(name.to_string());
        }
    }

    pub fn remove(&self, name: &str) {
        self.disabled
            .write()
            .expect("skill disabled lock")
            .remove(name);
        let mut index = self.full_index.write().expect("skill index lock");
        let skills: Vec<SkillDocument> = index
            .skills()
            .iter()
            .filter(|s| s.name != name)
            .cloned()
            .collect();
        *index = SkillIndex::new(skills);
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        !self
            .disabled
            .read()
            .expect("skill disabled lock")
            .contains(name)
    }

    /// 供 `LlmAgentBuilder::with_skills` 使用（已过滤禁用项）。
    pub fn agent_index(&self) -> SkillIndex {
        let full = self.full_index.read().expect("skill index lock");
        let disabled = self.disabled.read().expect("skill disabled lock");
        let skills: Vec<SkillDocument> = full
            .skills()
            .iter()
            .filter(|s| !disabled.contains(&s.name))
            .cloned()
            .collect();
        SkillIndex::new(skills)
    }

    pub fn full_index(&self) -> SkillIndex {
        self.full_index.read().expect("skill index lock").clone()
    }

    pub fn find_by_name(&self, name: &str) -> Option<SkillDocument> {
        self.full_index().find_by_name(name).cloned()
    }

    pub fn enabled_count(&self) -> usize {
        let full = self.full_index.read().expect("skill index lock");
        let disabled = self.disabled.read().expect("skill disabled lock");
        full.skills()
            .iter()
            .filter(|s| !disabled.contains(&s.name))
            .count()
    }

    pub fn total_count(&self) -> usize {
        self.full_index.read().expect("skill index lock").len()
    }
}

/// maco Agent 默认 Skill 选择策略：按用户消息自动匹配相关 Skill。
pub fn default_selection_policy() -> SelectionPolicy {
    SelectionPolicy {
        top_k: 2,
        min_score: 0.15,
        ..SelectionPolicy::default()
    }
}
