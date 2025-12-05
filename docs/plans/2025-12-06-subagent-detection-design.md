# Sub-agent Detection Design

## Problem

When Claude Code spawns sub-agents via the Task tool, each sub-agent appears as a separate session in the viewer. This creates duplicate/multiple entries for what is logically a single session.

## Solution

Detect sub-agent JSONL files, match them to their parent session by `sessionId`, and display an inline badge showing the count of active sub-agents.

## Detection

Sub-agent files have these characteristics:
- Filename pattern: `agent-*.jsonl` (vs UUID-named main sessions)
- Contains `"agentId": "..."` field in messages
- Contains `"isSidechain": true` in messages
- Shares the same `sessionId` as the parent session

## Data Model Changes

### Rust (`src-tauri/src/session/model.rs`)

Add to `Session` struct:
```rust
pub active_subagent_count: usize,
```

### TypeScript (`src/types/session.ts`)

Add to `Session` interface:
```typescript
activeSubagentCount: number;
```

## Parsing Logic (`src-tauri/src/session/parser.rs`)

1. **Filter out sub-agent files early** - exclude `agent-*.jsonl` from main session detection to prevent duplicate entries

2. **Count active sub-agents** - new function:
```rust
fn count_active_subagents(
    project_dir: &PathBuf,
    parent_session_id: &str,
    active_threshold_secs: u64,
) -> usize
```
   - Find all `agent-*.jsonl` files in project directory
   - Filter to recently modified (within 30 seconds)
   - Parse each to extract `sessionId`
   - Count those matching the parent session

3. **Populate count** - call `count_active_subagents` when building each `Session`

## Frontend UI

Display inline badge when sub-agents are active:
```tsx
{session.activeSubagentCount > 0 && (
  <span className="text-xs text-muted-foreground ml-2">
    [+{session.activeSubagentCount}]
  </span>
)}
```

Placement: After session name or status indicator, subtle but visible.
