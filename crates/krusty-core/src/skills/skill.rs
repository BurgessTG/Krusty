//! Skill data structures and parsing

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where the skill comes from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillSource {
    /// Global skills from ~/.krusty/skills/
    Global,
    /// Project-specific skills from .krusty/skills/
    Project,
}

/// Skill metadata for listing/discovery (lightweight)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub source: SkillSource,
}

/// YAML frontmatter from SKILL.md
#[derive(Debug, Clone, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Full skill with content loaded
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub source: SkillSource,
    /// Path to skill directory
    pub path: PathBuf,
    /// SKILL.md content (without frontmatter)
    pub content: String,
}

impl Skill {
    /// Parse SKILL.md content into a Skill
    pub fn parse(content: &str, path: PathBuf, source: SkillSource) -> Result<Self> {
        let (frontmatter, body) = parse_frontmatter(content)?;

        Ok(Self {
            name: frontmatter.name,
            description: frontmatter.description,
            version: frontmatter.version,
            author: frontmatter.author,
            tags: frontmatter.tags,
            source,
            path,
            content: body,
        })
    }

    /// Convert to lightweight SkillInfo
    pub fn to_info(&self) -> SkillInfo {
        SkillInfo {
            name: self.name.clone(),
            description: self.description.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            tags: self.tags.clone(),
            source: self.source,
        }
    }

    /// Get full content with frontmatter stripped (for AI context)
    pub fn get_content(&self) -> &str {
        &self.content
    }
}

/// Parse YAML frontmatter from markdown content
fn parse_frontmatter(content: &str) -> Result<(SkillFrontmatter, String)> {
    let content = content.trim();

    // Check for frontmatter delimiter
    if !content.starts_with("---") {
        return Err(anyhow!("SKILL.md must start with YAML frontmatter (---)"));
    }

    // Find closing delimiter
    let rest = &content[3..];
    let end_pos = rest
        .find("\n---")
        .ok_or_else(|| anyhow!("Missing closing frontmatter delimiter (---)"))?;

    let yaml_content = &rest[..end_pos].trim();
    let body = &rest[end_pos + 4..].trim();

    // Parse YAML
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_content)
        .map_err(|e| anyhow!("Failed to parse SKILL.md frontmatter: {}", e))?;

    // Validate required fields
    if frontmatter.name.is_empty() {
        return Err(anyhow!("Skill name cannot be empty"));
    }
    if frontmatter.description.is_empty() {
        return Err(anyhow!("Skill description cannot be empty"));
    }

    // Validate name format (lowercase, numbers, hyphens only)
    if !frontmatter
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(anyhow!(
            "Skill name must contain only lowercase letters, numbers, and hyphens"
        ));
    }

    Ok((frontmatter, body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_skill() {
        let content = r#"---
name: git-commit
description: Generate descriptive commit messages
version: 1.0.0
author: krusty
tags:
  - git
  - workflow
---

# Git Commit Helper

This skill helps generate commit messages.
"#;

        let skill = Skill::parse(content, PathBuf::from("/test"), SkillSource::Global).unwrap();
        assert_eq!(skill.name, "git-commit");
        assert_eq!(skill.description, "Generate descriptive commit messages");
        assert_eq!(skill.version, Some("1.0.0".to_string()));
        assert!(skill.content.contains("Git Commit Helper"));
    }

    #[test]
    fn test_parse_minimal_skill() {
        let content = r#"---
name: simple
description: A simple skill
---

Content here.
"#;

        let skill = Skill::parse(content, PathBuf::from("/test"), SkillSource::Project).unwrap();
        assert_eq!(skill.name, "simple");
        assert!(skill.tags.is_empty());
    }

    #[test]
    fn test_invalid_name() {
        let content = r#"---
name: Invalid Name
description: Test
---
"#;

        let result = Skill::parse(content, PathBuf::from("/test"), SkillSource::Global);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_frontmatter() {
        let content = "# No frontmatter";
        let result = Skill::parse(content, PathBuf::from("/test"), SkillSource::Global);
        assert!(result.is_err());
    }
}
