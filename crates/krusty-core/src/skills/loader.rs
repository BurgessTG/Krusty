//! Skill filesystem loading

use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::skill::{Skill, SkillSource};

/// Load all skills from a directory
pub fn load_skills_from_dir(dir: &Path, source: SkillSource) -> Vec<Skill> {
    let mut skills = Vec::new();

    if !dir.exists() || !dir.is_dir() {
        return skills;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return skills;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let skill_file = path.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }

        match load_skill(&path, source) {
            Ok(skill) => {
                debug!("Loaded skill: {} from {:?}", skill.name, path);
                skills.push(skill);
            }
            Err(e) => {
                debug!("Failed to load skill from {:?}: {}", path, e);
            }
        }
    }

    // Sort by name
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Load a single skill from its directory
pub fn load_skill(path: &Path, source: SkillSource) -> Result<Skill> {
    let skill_file = path.join("SKILL.md");

    if !skill_file.exists() {
        return Err(anyhow!("SKILL.md not found in {:?}", path));
    }

    let content = fs::read_to_string(&skill_file)
        .map_err(|e| anyhow!("Failed to read SKILL.md: {}", e))?;

    Skill::parse(&content, path.to_path_buf(), source)
}

/// Load a specific file from within a skill directory
pub fn load_skill_file(skill_path: &Path, file_name: &str) -> Result<String> {
    // Prevent path traversal
    if file_name.contains("..") {
        return Err(anyhow!("Invalid file path: path traversal not allowed"));
    }

    let file_path = skill_path.join(file_name);

    // Ensure file is within skill directory
    let canonical_skill = skill_path.canonicalize()?;
    let canonical_file = file_path.canonicalize()?;

    if !canonical_file.starts_with(&canonical_skill) {
        return Err(anyhow!("File path escapes skill directory"));
    }

    if !file_path.exists() {
        return Err(anyhow!("File not found: {}", file_name));
    }

    fs::read_to_string(&file_path)
        .map_err(|e| anyhow!("Failed to read {}: {}", file_name, e))
}

/// Ensure skills directory exists
pub fn ensure_skills_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Scaffold a new skill directory
pub fn scaffold_skill(dir: &Path, name: &str, description: &str) -> Result<PathBuf> {
    let skill_dir = dir.join(name);

    if skill_dir.exists() {
        return Err(anyhow!("Skill '{}' already exists", name));
    }

    fs::create_dir_all(&skill_dir)?;

    let skill_content = format!(
        r#"---
name: {}
description: {}
version: 0.1.0
---

# {}

## Quick Start

[Add quick start instructions here]

## Usage

[Add usage instructions here]

## Examples

[Add examples here]
"#,
        name,
        description,
        name.split('-').map(capitalize_first).collect::<Vec<_>>().join(" ")
    );

    fs::write(skill_dir.join("SKILL.md"), skill_content)?;

    Ok(skill_dir)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_scaffold_skill() {
        let temp = tempdir().unwrap();
        let result = scaffold_skill(temp.path(), "test-skill", "A test skill");
        assert!(result.is_ok());

        let skill_dir = result.unwrap();
        assert!(skill_dir.join("SKILL.md").exists());

        // Verify it can be loaded
        let skill = load_skill(&skill_dir, SkillSource::Global).unwrap();
        assert_eq!(skill.name, "test-skill");
    }

    #[test]
    fn test_path_traversal_blocked() {
        let temp = tempdir().unwrap();
        let skill_dir = temp.path().join("skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: x\ndescription: x\n---").unwrap();

        let result = load_skill_file(&skill_dir, "../../../etc/passwd");
        assert!(result.is_err());
    }
}
