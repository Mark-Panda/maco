//! Skill 安装与删除：上传 zip 解压到 `~/.maco/skills/`。

use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use maco_core::{MacoError, MacoResult, default_skills_dir};
use uuid::Uuid;
use zip::ZipArchive;

use adk_skill::{SkillDocument, load_skill_index_with_extras, parse_skill_markdown};

/// Skill zip 包最大体积（20 MB）。
pub const MAX_SKILL_ZIP_BYTES: usize = 20 * 1024 * 1024;
/// 解压后累计最大体积（50 MB），防止 zip bomb。
const MAX_SKILL_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024;

/// zip 安装结果。
#[derive(Debug, Clone)]
pub struct SkillInstallResult {
    /// 安装后的 Skill 元数据。
    pub skill: SkillDocument,
    /// 解压出的文件数（不含目录）。
    pub extracted_files: usize,
}

/// 将 zip 解压并安装到 skills 目录。
pub fn install_skill_zip(
    bytes: &[u8],
    zip_filename: &str,
    overwrite: bool,
) -> MacoResult<SkillInstallResult> {
    if bytes.is_empty() {
        return Err(MacoError::config("empty zip file"));
    }
    if bytes.len() > MAX_SKILL_ZIP_BYTES {
        return Err(MacoError::config(format!(
            "zip exceeds {} MB limit",
            MAX_SKILL_ZIP_BYTES / 1024 / 1024
        )));
    }

    let skills_dir = default_skills_dir();
    std::fs::create_dir_all(&skills_dir)
        .map_err(|e| MacoError::config(format!("create skills dir: {e}")))?;
    let install_root = skills_dir.join(".install");
    std::fs::create_dir_all(&install_root)
        .map_err(|e| MacoError::config(format!("create install dir: {e}")))?;

    let temp_dir = install_root.join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| MacoError::config(format!("create temp dir: {e}")))?;

    let cleanup = TempDirGuard(temp_dir.clone());
    let extracted_files = extract_zip_secure(bytes, &temp_dir)?;
    if extracted_files == 0 {
        return Err(MacoError::config(
            "zip contains no installable files (need at least one .md skill file)",
        ));
    }

    let zip_stem = Path::new(zip_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("skill");
    let content_root = resolve_content_root(&temp_dir)?;
    let install_name = resolve_install_name(&content_root, &temp_dir, zip_stem)?;
    let target_dir = skills_dir.join(&install_name);

    if target_dir.exists() {
        if !overwrite {
            return Err(MacoError::config(format!(
                "skill already exists: {install_name} (pass overwrite=true to replace)"
            )));
        }
        remove_path_secure(&target_dir, &skills_dir)?;
    }

    if content_root == temp_dir {
        std::fs::rename(&temp_dir, &target_dir)
            .map_err(|e| MacoError::config(format!("install skill dir: {e}")))?;
        std::mem::forget(cleanup);
    } else {
        std::fs::rename(&content_root, &target_dir)
            .map_err(|e| MacoError::config(format!("install skill dir: {e}")))?;
    }

    let skill = find_skill_by_name(&install_name).ok_or_else(|| {
        MacoError::config(format!(
            "installed to {install_name} but no valid ADK skill markdown was found (need frontmatter name + description)"
        ))
    })?;

    Ok(SkillInstallResult {
        skill,
        extracted_files,
    })
}

/// 按名称删除已安装的 Skill（文件或目录）。
pub fn delete_skill(name: &str) -> MacoResult<()> {
    let skill = find_skill_by_name(name).ok_or_else(|| MacoError::not_found("skill"))?;
    let skills_dir = default_skills_dir();
    let target = skill_deletion_path(&skill.path, &skills_dir);
    if !path_within(&target, &skills_dir) {
        return Err(MacoError::config(
            "refusing to delete path outside skills dir",
        ));
    }
    if !target.exists() {
        return Err(MacoError::not_found("skill"));
    }
    remove_path_secure(&target, &skills_dir)
}

fn find_skill_by_name(name: &str) -> Option<SkillDocument> {
    let skills_dir = default_skills_dir();
    let index =
        load_skill_index_with_extras(&skills_dir, std::slice::from_ref(&skills_dir)).ok()?;
    index.find_by_name(name).cloned()
}

/// Skill 在磁盘上的安装根路径（目录或单文件）。
pub fn skill_deletion_path(skill_path: &Path, skills_dir: &Path) -> PathBuf {
    let is_skill_md = skill_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("SKILL.md"));
    if is_skill_md {
        return skill_path.parent().unwrap_or(skill_path).to_path_buf();
    }
    if skill_path.parent().is_some_and(|p| p == skills_dir) {
        return skill_path.to_path_buf();
    }
    skill_path.parent().unwrap_or(skill_path).to_path_buf()
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn extract_zip_secure(bytes: &[u8], dest: &Path) -> MacoResult<usize> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| MacoError::config(format!("invalid zip: {e}")))?;
    let mut extracted_files = 0usize;
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| MacoError::config(format!("read zip entry: {e}")))?;
        let Some(relative) = file.enclosed_name() else {
            return Err(MacoError::config("zip entry has an unsafe path"));
        };
        if !is_safe_relative_path(&relative) {
            return Err(MacoError::config("zip entry has an unsafe path"));
        }
        if should_ignore_zip_path(&relative) {
            continue;
        }

        total_uncompressed = total_uncompressed.saturating_add(file.size());
        if total_uncompressed > MAX_SKILL_UNCOMPRESSED_BYTES {
            return Err(MacoError::config("extracted content exceeds size limit"));
        }

        let out_path = dest.join(&relative);

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| MacoError::config(format!("create dir: {e}")))?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MacoError::config(format!("create parent dir: {e}")))?;
        }
        let mut outfile = std::fs::File::create(&out_path)
            .map_err(|e| MacoError::config(format!("create file: {e}")))?;
        std::io::copy(&mut file, &mut outfile)
            .map_err(|e| MacoError::config(format!("write file: {e}")))?;
        extracted_files += 1;
    }

    Ok(extracted_files)
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn should_ignore_zip_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.starts_with("__MACOSX/") || s.contains("/__MACOSX/") {
        return true;
    }
    path.components().any(|c| match c {
        Component::Normal(name) => {
            let n = name.to_string_lossy();
            n.starts_with('.') || n == ".DS_Store"
        }
        _ => false,
    })
}

fn resolve_content_root(temp_dir: &Path) -> MacoResult<PathBuf> {
    let entries = list_visible_entries(temp_dir)?;
    if entries.is_empty() {
        return Err(MacoError::config("zip is empty"));
    }
    if entries.len() == 1
        && entries[0]
            .file_type()
            .map_err(|e| MacoError::config(format!("read entry type: {e}")))?
            .is_dir()
    {
        Ok(entries[0].path())
    } else {
        Ok(temp_dir.to_path_buf())
    }
}

fn resolve_install_name(
    content_root: &Path,
    temp_dir: &Path,
    zip_stem: &str,
) -> MacoResult<String> {
    if let Some(skill_md) = find_primary_skill_md(content_root)
        && let Ok(raw) = std::fs::read_to_string(&skill_md)
        && let Ok(parsed) = parse_skill_markdown(&skill_md, &raw)
    {
        return Ok(parsed.name);
    }
    if content_root != temp_dir
        && let Some(name) = content_root.file_name().and_then(|s| s.to_str())
        && !name.is_empty()
    {
        return Ok(name.to_string());
    }
    Ok(zip_stem.to_string())
}

fn find_primary_skill_md(root: &Path) -> Option<PathBuf> {
    let skill_md = root.join("SKILL.md");
    if skill_md.is_file() {
        return Some(skill_md);
    }
    let entries = list_visible_entries(root).ok()?;
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_primary_skill_md(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            return Some(path);
        }
    }
    None
}

fn list_visible_entries(dir: &Path) -> MacoResult<Vec<std::fs::DirEntry>> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| MacoError::config(format!("read dir: {e}")))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().is_some_and(|n| !n.starts_with('.')))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

fn path_within(path: &Path, root: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else if let Some(parent) = path.parent() {
        parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(path.file_name().unwrap_or_default())
    } else {
        path.to_path_buf()
    };
    path.starts_with(&root)
}

fn remove_path_secure(path: &Path, skills_dir: &Path) -> MacoResult<()> {
    if !path_within(path, skills_dir) {
        return Err(MacoError::config(
            "refusing to delete path outside skills dir",
        ));
    }
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| MacoError::config(format!("remove dir: {e}")))?;
    } else if path.is_file() {
        std::fs::remove_file(path).map_err(|e| MacoError::config(format!("remove file: {e}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn write_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();
        for (name, content) in entries {
            zip.start_file(*name, options).expect("start");
            zip.write_all(content.as_bytes()).expect("write");
        }
        zip.finish().expect("finish");
        buf
    }

    #[test]
    fn install_skill_zip_from_folder() {
        let skills_home = TempDir::new().expect("tmpdir");
        let skills_dir = skills_home.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");

        // Monkey-patch via installing to custom dir is not supported; test extract helpers only.
        let temp = TempDir::new().expect("tmpdir");
        let zip = write_zip(&[(
            "my-skill/SKILL.md",
            "---\nname: my-skill\ndescription: Demo skill\n---\n\n# Body\n",
        )]);
        let extracted = extract_zip_secure(&zip, temp.path()).expect("extract");
        assert_eq!(extracted, 1);
        let root = resolve_content_root(temp.path()).expect("root");
        assert!(root.join("SKILL.md").is_file());
        let name = resolve_install_name(&root, temp.path(), "upload").expect("name");
        assert_eq!(name, "my-skill");
    }
}
