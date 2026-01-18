# Krusty Audit TODO

**Audit Date:** January 18, 2026
**Last Verified:** January 18, 2026
**Overall Score:** 7.8/10 — Production-ready with targeted improvements

---

## ✅ Phase 1 Completed (Jan 18, 2026)

### Grep ReDoS Protection ✅
- [x] Added `validate_pattern()` function with MAX_PATTERN_LENGTH constant
- [x] Rejects patterns > 1000 chars
- [x] Detects nested quantifiers like `(a+)+`, `(a*)*`
- [x] 4 unit tests added

### LSP Health Monitoring ✅
- [x] Added `is_healthy()` to LspClient (uses AtomicBool)
- [x] Added `health_check()` to LspManager
- [x] Added `restart_server()` capability
- [x] Extracted `DIAGNOSTICS_WAIT_MS` constant (150ms)

### SSE Integration Tests ✅
- [x] 28 tests added to sse.rs:
  - `parse_finish_reason` (5 tests)
  - `ToolCallAccumulator` (8 tests)
  - `ServerToolAccumulator` (5 tests)
  - `ThinkingAccumulator` (5 tests)
  - `SseStreamProcessor` (5 tests)

### SSE Performance Optimization ✅
- [x] Replaced `lines().collect()` with peekable iterator (avoids Vec allocation)
- [x] Optimized partial line handling with `std::mem::take()`
- [x] Added `String::with_capacity(256)` in StreamBuffer::new()

---

## ✅ Previously Completed

### Centralized Reasoning Config ✅
- [x] Created `reasoning.rs` with `ReasoningConfig` builder
- [x] Handles Anthropic, OpenAI, DeepSeek formats
- [x] 11 unit tests

### Parser Extraction ✅
- [x] Extracted `AnthropicParser` to `parsers/anthropic.rs`
- [x] Extracted `OpenAIParser` to `parsers/openai.rs`
- [x] `client.rs` reduced from 2381 to 1393 lines

### Path Validation Utility ✅
- [x] Created `tools/path_utils.rs`
- [x] `validate_path()` and `validate_new_path()` functions

### OAuth Removal ✅
- [x] Deleted `auth/oauth.rs` and `auth/token_manager.rs`
- [x] Simplified auth popup to API-key only

---

## ✅ Phase 2 Completed (Jan 18, 2026)

### Use ReasoningConfig in client.rs ✅
- [x] Replaced inline max_tokens calculation with `ReasoningConfig::max_tokens_for_format()`
- [x] Replaced inline reasoning config building with `ReasoningConfig::build()`
- [x] Uses `ReasoningConfig::build_opus_effort()` for Opus 4.5 effort config
- [x] Note: Original audit incorrectly identified transform.rs; actual duplication was in client.rs

### Google API Format for Gemini ✅
- [x] Added `uses_google_format()` helper method to ClientConfig
- [x] Fixed URL endpoint to include `:streamGenerateContent` for streaming
- [x] Added routing for Google format in `call_streaming()`
- [x] Implemented `call_streaming_google()` method
- [x] Implemented `convert_messages_google()` - converts messages to Google contents/parts format
- [x] Implemented `convert_tools_google()` - converts tools to Google function declarations
- [x] Created `parsers/google.rs` with `GoogleParser` for streaming response parsing

---

## 🟡 Phase 2b: Architectural Changes (Deferred - Requires Focused Sprint)

These items are significant architectural changes analyzed on Jan 18, 2026.

### 1. Split App Struct (God Object)
**Location:** `crates/krusty-cli/src/tui/app.rs`
**Current State:** 55 fields, 1,864 lines
**Analysis Completed:**
- Fields naturally group into 7 logical categories:
  - AI/Model State (8 fields): current_model, ai_client, api_key, etc.
  - Session State (6 fields): session_manager, working_dir, preferences, etc.
  - Processing State (6 fields): is_streaming, streaming, current_activity, etc.
  - LSP State (3 fields): lsp_manager, lsp_skip_list, pending_lsp_install
  - Process State (3 fields): process_registry, running_process_count, etc.
  - Tool State (5 fields): tool_registry, cached_ai_tools, queued_tools, etc.
  - Agent State (4 fields): event_bus, agent_state, agent_config, cancellation
**Impact:** Session state alone has 78 references across 12 files
**Recommendation:** Defer - high effort, marginal benefit. Fields are already well-organized with comments. Consider if needed when adding new features.

### 2. Complete BlockManager Phase-out
**Location:** `crates/krusty-cli/src/tui/app.rs:226`
**Current State:** 140 references to `self.blocks.` across 12 files
**Analysis Completed:**
- BlockManager stores block instances (ThinkingBlock, BashBlock, etc.) in Vec collections
- New approach uses ID-based state (BlockUiStates + ToolResultCache)
- Migration requires changing rendering to read from conversation instead of blocks
**Files Affected:** (sorted by impact)
  1. `handlers/mouse.rs` - 38 refs
  2. `handlers/rendering/messages.rs` - 24 refs
  3. `handlers/sessions.rs` - 16 refs
  4. `app.rs` - 15 refs
  5. `handlers/streaming.rs` - 11 refs
  6. `handlers/stream_events.rs` - 10 refs
  7. `handlers/selection.rs` - 9 refs
  8. `handlers/scrollbar.rs` - 7 refs
  9. `handlers/rendering/views.rs` - 4 refs
  10. `handlers/keyboard.rs` - 3 refs
  11. `handlers/commands.rs` - 2 refs
  12. `handlers/hit_test.rs` - 1 ref
**Recommendation:** Defer - requires focused sprint with comprehensive testing. Current implementation works.

---

## ✅ Phase 3: Security Hardening (Partial)

### File Size Limits ✅
- [x] Added `MAX_FILE_SIZE` (10 MB) to read.rs - prevents memory exhaustion
- [x] Added `MAX_WRITE_SIZE` (10 MB) to write.rs - prevents disk exhaustion
- [x] Error messages guide users to use offset/limit for large files

### Remaining Security Items
- [ ] Add bounded channels (8 files use unbounded_channel)
- [ ] MCP tool sandboxing
- [ ] Windows file permissions

---

## 📈 Test Coverage

| Area | Status |
|------|--------|
| SSE parsing | ✅ 28 tests |
| reasoning.rs | ✅ 11 tests |
| grep validation | ✅ 4 tests |
| Tool execution | 🔴 No integration tests |
| LSP lifecycle | 🔴 No tests |

---

## Current Build Status

```
cargo test -p krusty-core: 102 tests pass ✅
cargo check: Passes
cargo clippy: Clean
```
