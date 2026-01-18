//! Skills system for extending Claude's capabilities
//!
//! Skills are modular, filesystem-based resources that provide Claude with
//! domain-specific expertise: workflows, context, and best practices.
//!
//! # Directory Structure
//!
//! Skills are stored in two locations:
//! - Global: `~/.krusty/skills/` - Available across all sessions
//! - Project: `.krusty/skills/` - Project-specific skills
//!
//! Each skill is a directory containing a `SKILL.md` file with YAML frontmatter:
//!
//! ```yaml
//! ---
//! name: skill-name
//! description: Brief description for discovery
//! version: 1.0.0
//! ---
//!
//! # Skill Name
//!
//! [Instructions...]
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use krusty_core::skills::SkillsManager;
//!
//! let mut manager = SkillsManager::with_defaults(&working_dir);
//!
//! // List available skills
//! for skill in manager.list_skills() {
//!     println!("{}: {}", skill.name, skill.description);
//! }
//!
//! // Load skill content for AI context
//! let content = manager.load_skill_content("git-commit")?;
//! ```

mod loader;
mod manager;
mod skill;

pub use manager::SkillsManager;
pub use skill::{Skill, SkillInfo, SkillSource};

// Re-export loader functions for direct use
pub use loader::{scaffold_skill, load_skill, load_skill_file};
