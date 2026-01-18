//! Session CRUD operations

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::database::Database;
use crate::agent::PinchContext;

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub updated_at: DateTime<Utc>,
    pub token_count: Option<usize>,
    /// Parent session ID for linked sessions (pinch)
    pub parent_session_id: Option<String>,
    /// Working directory for this session
    pub working_dir: Option<String>,
    /// User ID for multi-tenant isolation
    pub user_id: Option<String>,
}

/// Session manager for CRUD operations
pub struct SessionManager {
    db: Database,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Get reference to underlying database
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Create a new session
    pub fn create_session(
        &self,
        title: &str,
        model: Option<&str>,
        working_dir: Option<&str>,
    ) -> Result<String> {
        self.create_session_for_user(title, model, working_dir, None)
    }

    /// Create a new session with user ownership (multi-tenant)
    pub fn create_session_for_user(
        &self,
        title: &str,
        model: Option<&str>,
        working_dir: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.db.conn().execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, model, working_dir, user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, title, now, now, model, working_dir, user_id],
        )?;

        Ok(id)
    }

    /// List sessions, optionally filtered by working directory
    ///
    /// If `working_dir` is Some, only returns sessions from that directory.
    /// If None, returns all sessions.
    pub fn list_sessions(&self, working_dir: Option<&str>) -> Result<Vec<SessionInfo>> {
        self.list_sessions_for_user(working_dir, None)
    }

    /// List sessions for a specific user (multi-tenant)
    ///
    /// If `user_id` is Some, only returns sessions owned by that user.
    /// If `working_dir` is Some, also filters by that directory.
    pub fn list_sessions_for_user(
        &self,
        working_dir: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<Vec<SessionInfo>> {
        match (working_dir, user_id) {
            (Some(dir), Some(uid)) => {
                let mut stmt = self.db.conn().prepare(
                    "SELECT id, title, updated_at, token_count, parent_session_id, working_dir, user_id
                     FROM sessions WHERE working_dir = ?1 AND user_id = ?2
                     ORDER BY updated_at DESC",
                )?;
                let sessions = stmt
                    .query_map(params![dir, uid], Self::map_session_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(sessions)
            }
            (Some(dir), None) => {
                let mut stmt = self.db.conn().prepare(
                    "SELECT id, title, updated_at, token_count, parent_session_id, working_dir, user_id
                     FROM sessions WHERE working_dir = ?1
                     ORDER BY updated_at DESC",
                )?;
                let sessions = stmt
                    .query_map([dir], Self::map_session_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(sessions)
            }
            (None, Some(uid)) => {
                let mut stmt = self.db.conn().prepare(
                    "SELECT id, title, updated_at, token_count, parent_session_id, working_dir, user_id
                     FROM sessions WHERE user_id = ?1
                     ORDER BY updated_at DESC",
                )?;
                let sessions = stmt
                    .query_map([uid], Self::map_session_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(sessions)
            }
            (None, None) => {
                let mut stmt = self.db.conn().prepare(
                    "SELECT id, title, updated_at, token_count, parent_session_id, working_dir, user_id
                     FROM sessions ORDER BY updated_at DESC",
                )?;
                let sessions = stmt
                    .query_map([], Self::map_session_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(sessions)
            }
        }
    }

    /// Helper to map a row to SessionInfo
    fn map_session_row(row: &rusqlite::Row) -> rusqlite::Result<SessionInfo> {
        let updated_at: String = row.get(2)?;
        let token_count: Option<i64> = row.get(3)?;

        Ok(SessionInfo {
            id: row.get(0)?,
            title: row.get(1)?,
            updated_at: DateTime::parse_from_rfc3339(&updated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            token_count: token_count.map(|t| t as usize),
            parent_session_id: row.get(4)?,
            working_dir: row.get(5)?,
            user_id: row.get(6)?,
        })
    }

    /// List all directories that have sessions
    ///
    /// Returns sorted list of unique working directories.
    pub fn list_session_directories(&self) -> Result<Vec<String>> {
        self.list_session_directories_for_user(None)
    }

    /// List directories for a specific user (multi-tenant)
    pub fn list_session_directories_for_user(&self, user_id: Option<&str>) -> Result<Vec<String>> {
        if let Some(uid) = user_id {
            let mut stmt = self.db.conn().prepare(
                "SELECT DISTINCT working_dir FROM sessions
                 WHERE working_dir IS NOT NULL AND user_id = ?1
                 ORDER BY working_dir",
            )?;
            let dirs = stmt.query_map([uid], |row| row.get(0))?;
            dirs.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        } else {
            let mut stmt = self.db.conn().prepare(
                "SELECT DISTINCT working_dir FROM sessions
                 WHERE working_dir IS NOT NULL
                 ORDER BY working_dir",
            )?;
            let dirs = stmt.query_map([], |row| row.get(0))?;
            dirs.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        }
    }

    /// Verify session belongs to user (multi-tenant ownership check)
    ///
    /// Returns true if the session exists and belongs to the specified user.
    /// Returns true for any session if user_id is None (single-tenant mode).
    pub fn verify_session_ownership(&self, session_id: &str, user_id: Option<&str>) -> Result<bool> {
        if let Some(uid) = user_id {
            let count: i64 = self.db.conn().query_row(
                "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND user_id = ?2",
                params![session_id, uid],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        } else {
            // Single-tenant mode - just check session exists
            let count: i64 = self.db.conn().query_row(
                "SELECT COUNT(*) FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        }
    }

    /// Get sessions grouped by directory
    ///
    /// Returns a map of directory -> sessions for tree display.
    pub fn list_sessions_by_directory(&self) -> Result<std::collections::HashMap<String, Vec<SessionInfo>>> {
        use std::collections::HashMap;

        let mut stmt = self.db.conn().prepare(
            "SELECT id, title, updated_at, token_count, parent_session_id, working_dir, user_id
             FROM sessions
             WHERE working_dir IS NOT NULL
             ORDER BY working_dir, updated_at DESC",
        )?;

        let mut result: HashMap<String, Vec<SessionInfo>> = HashMap::new();

        let rows = stmt.query_map([], |row| {
            let updated_at: String = row.get(2)?;
            let token_count: Option<i64> = row.get(3)?;
            let working_dir: String = row.get(5)?;

            Ok((
                working_dir.clone(),
                SessionInfo {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    updated_at: DateTime::parse_from_rfc3339(&updated_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    token_count: token_count.map(|t| t as usize),
                    parent_session_id: row.get(4)?,
                    working_dir: Some(working_dir),
                    user_id: row.get(6)?,
                },
            ))
        })?;

        for row in rows {
            let (dir, session) = row?;
            result.entry(dir).or_default().push(session);
        }

        Ok(result)
    }

    /// Get a specific session
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, title, updated_at, token_count, parent_session_id, working_dir, user_id FROM sessions WHERE id = ?1",
        )?;

        let session = stmt.query_row([session_id], |row| {
            let updated_at: String = row.get(2)?;
            let token_count: Option<i64> = row.get(3)?;

            Ok(SessionInfo {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                token_count: token_count.map(|t| t as usize),
                parent_session_id: row.get(4)?,
                working_dir: row.get(5)?,
                user_id: row.get(6)?,
            })
        });

        match session {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update session title
    pub fn update_session_title(&self, session_id: &str, title: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        self.db.conn().execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, session_id],
        )?;

        Ok(())
    }

    /// Update session token count (silently fails if column doesn't exist yet)
    pub fn update_token_count(&self, session_id: &str, token_count: usize) {
        // This may fail if migration hasn't run yet - that's ok
        let _ = self.db.conn().execute(
            "UPDATE sessions SET token_count = ?1 WHERE id = ?2",
            params![token_count as i64, session_id],
        );
    }

    /// Delete a session and all its messages
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        // First, clear parent_session_id references from children (orphan them)
        // This prevents foreign key constraint violations
        let _ = self.db.conn().execute(
            "UPDATE sessions SET parent_session_id = NULL WHERE parent_session_id = ?1",
            params![session_id],
        );

        // Clear pinch_metadata references
        let _ = self.db.conn().execute(
            "DELETE FROM pinch_metadata WHERE source_session_id = ?1 OR target_session_id = ?1",
            params![session_id],
        );

        // Clear file_activity for this session
        let _ = self.db.conn().execute(
            "DELETE FROM file_activity WHERE session_id = ?1",
            params![session_id],
        );

        // Clear block_ui_state for this session
        let _ = self.db.conn().execute(
            "DELETE FROM block_ui_state WHERE session_id = ?1",
            params![session_id],
        );

        // Messages will be deleted via ON DELETE CASCADE
        self.db
            .conn()
            .execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;

        tracing::info!(session_id = %session_id, "Session deleted from database");
        Ok(())
    }

    /// Save a message to a session
    /// The content field stores JSON-serialized Vec<Content> for full fidelity
    pub fn save_message(&self, session_id: &str, role: &str, content_json: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        self.db.conn().execute(
            "INSERT INTO messages (session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role, content_json, now],
        )?;

        // Update session timestamp
        self.db.conn().execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;

        Ok(())
    }

    /// Load all messages for a session
    /// Returns (role, content_json) pairs where content_json can be deserialized to Vec<Content>
    pub fn load_session_messages(&self, session_id: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .db
            .conn()
            .prepare("SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY id")?;

        let messages = stmt.query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        messages.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Generate a title from the first message content
    /// Truncates at word boundaries for cleaner display
    /// Uses char-based indexing for UTF-8 safety
    pub fn generate_title_from_content(content: &str) -> String {
        // Use first line only, cleaned up
        let first_line = content.lines().next().unwrap_or("").trim();

        // Count chars (not bytes) for UTF-8 safety
        let char_count = first_line.chars().count();

        // If short enough, use as-is
        if char_count <= 50 {
            return first_line.to_string();
        }

        // Get first 50 chars and find last word boundary
        let first_50: String = first_line.chars().take(50).collect();
        if let Some(last_space) = first_50.rfind(char::is_whitespace) {
            // last_space is a byte index in first_50, but first_50 is already truncated
            // So we can safely slice it
            let char_idx = first_50[..last_space].chars().count();
            if char_idx > 20 {
                // Only use word boundary if we keep at least 20 chars
                let prefix: String = first_line.chars().take(char_idx).collect();
                return format!("{}...", prefix.trim_end());
            }
        }

        // Fallback: hard truncate at 47 chars
        let truncated: String = first_line.chars().take(47).collect();
        format!("{}...", truncated)
    }

    // =========================================================================
    // Block UI State
    // =========================================================================

    /// Save block UI state (collapsed, scroll_offset) for a block
    pub fn save_block_ui_state(
        &self,
        session_id: &str,
        block_id: &str,
        collapsed: bool,
        scroll_offset: u16,
    ) {
        // Silently fails if table doesn't exist yet (pre-migration 3)
        let _ = self.db.conn().execute(
            "INSERT OR REPLACE INTO block_ui_state (session_id, block_id, block_type, collapsed, scroll_offset)
             VALUES (?1, ?2, '', ?3, ?4)",
            params![session_id, block_id, collapsed as i32, scroll_offset as i32],
        );
    }

    /// Load all block UI states for a session
    pub fn load_block_ui_states(&self, session_id: &str) -> Vec<BlockUiState> {
        let result = (|| -> Result<Vec<BlockUiState>> {
            let mut stmt = self.db.conn().prepare(
                "SELECT block_id, collapsed, scroll_offset
                 FROM block_ui_state WHERE session_id = ?1",
            )?;

            let states = stmt.query_map([session_id], |row| {
                Ok(BlockUiState {
                    block_id: row.get(0)?,
                    collapsed: row.get::<_, i32>(1)? != 0,
                    scroll_offset: row.get::<_, i32>(2)? as u16,
                })
            })?;

            states.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })();

        result.unwrap_or_default()
    }

    // =========================================================================
    // Linked Sessions (Pinch)
    // =========================================================================

    /// Create a new session linked to a parent (for pinch)
    ///
    /// The new session starts fresh but with a reference to its parent
    /// and pinch metadata preserved for context.
    pub fn create_linked_session(
        &self,
        title: &str,
        parent_session_id: &str,
        pinch_ctx: &PinchContext,
        model: Option<&str>,
        working_dir: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // Create new session with parent reference
        self.db.conn().execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, model, working_dir, parent_session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, title, now, now, model, working_dir, parent_session_id],
        )?;

        // Store pinch metadata
        let pinch_id = uuid::Uuid::new_v4().to_string();
        let key_files_json = serde_json::to_string(&pinch_ctx.ranked_files)?;

        self.db.conn().execute(
            "INSERT INTO pinch_metadata (id, source_session_id, target_session_id, summary, key_files, user_preservation_hints, user_direction, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                pinch_id,
                parent_session_id,
                id,
                &pinch_ctx.work_summary,
                key_files_json,
                &pinch_ctx.preservation_hints,
                &pinch_ctx.direction,
                now
            ],
        )?;

        Ok(id)
    }

    // =========================================================================
    // Agent State Tracking (for background execution)
    // =========================================================================

    /// Set the agent execution state for a session
    ///
    /// Valid states: "idle", "streaming", "tool_executing", "awaiting_input", "error"
    pub fn set_agent_state(&self, session_id: &str, state: &str) {
        let now = Utc::now().to_rfc3339();

        // Update state and last_event_at
        // Set agent_started_at only when transitioning from idle
        let _ = self.db.conn().execute(
            "UPDATE sessions SET
                agent_state = ?1,
                agent_last_event_at = ?2,
                agent_started_at = CASE
                    WHEN agent_state = 'idle' AND ?1 != 'idle' THEN ?2
                    WHEN ?1 = 'idle' THEN NULL
                    ELSE agent_started_at
                END
             WHERE id = ?3",
            params![state, now, session_id],
        );
    }

    /// Get the agent state for a session
    pub fn get_agent_state(&self, session_id: &str) -> Option<AgentState> {
        let result = self.db.conn().query_row(
            "SELECT agent_state, agent_started_at, agent_last_event_at
             FROM sessions WHERE id = ?1",
            [session_id],
            |row| {
                Ok(AgentState {
                    state: row.get::<_, String>(0)?,
                    started_at: row.get::<_, Option<String>>(1)?,
                    last_event_at: row.get::<_, Option<String>>(2)?,
                })
            },
        );

        result.ok()
    }

    /// Update agent last_event_at timestamp (for keeping session "alive")
    pub fn touch_agent_event(&self, session_id: &str) {
        let now = Utc::now().to_rfc3339();
        let _ = self.db.conn().execute(
            "UPDATE sessions SET agent_last_event_at = ?1 WHERE id = ?2",
            params![now, session_id],
        );
    }

    /// List sessions with active agents (not idle)
    pub fn list_active_sessions(&self) -> Result<Vec<(String, AgentState)>> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, agent_state, agent_started_at, agent_last_event_at
             FROM sessions WHERE agent_state != 'idle'",
        )?;

        let sessions = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AgentState {
                    state: row.get::<_, String>(1)?,
                    started_at: row.get::<_, Option<String>>(2)?,
                    last_event_at: row.get::<_, Option<String>>(3)?,
                },
            ))
        })?;

        sessions.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// Agent execution state
#[derive(Debug, Clone)]
pub struct AgentState {
    /// Current state: "idle", "streaming", "tool_executing", "awaiting_input", "error"
    pub state: String,
    /// When the agent started processing
    pub started_at: Option<String>,
    /// Last event timestamp
    pub last_event_at: Option<String>,
}

/// Block UI state for session restoration
#[derive(Debug, Clone)]
pub struct BlockUiState {
    pub block_id: String,
    pub collapsed: bool,
    pub scroll_offset: u16,
}
