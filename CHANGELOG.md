# Changelog

## [0.2.0] - 2026-04-06

### Performance
- Share single system process scanner between Claude and OpenCode detectors (was scanning entire process table twice per poll)
- Pre-filter project directories by active process CWDs — skip scanning inactive projects entirely (was opening 267+ files in inactive directories)
- Cache git remote URL lookups — subprocess only runs once per project instead of every 2s poll
- Tail-seek large JSONL session files — read last 512KB instead of entire multi-MB file
- Remove unused `rusqlite` bundled dependency (eliminates SQLite C compilation from clean builds)
- Add release profile optimizations (LTO, single codegen unit, symbol stripping)

### Before/After
- Poll scan time: **22-29 seconds → 14-17ms** (1500x faster)
- First scan: **22 seconds → 672ms** (33x faster)

## [0.1.27] - 2026-03-09

### Fixed
- Show sessions as "Waiting" when Claude is blocked on `AskUserQuestion` user input instead of "Processing"

## [0.1.26] - 2026-03-01

### Fixed
- Fix project path resolution for directories with hyphens

## [0.1.25] - 2026-02-08

### Fixed
- Fix status flickering when multiple sessions run in the same project - idle sessions no longer pick up active status from sibling sessions

## [0.1.24] - 2026-02-06

### Added
- "Compacting" status shown when a session is compressing its conversation context

## [0.1.23] - 2026-02-06

### Fixed
- Simplify status detection to use message content as primary signal instead of file age heuristics
- Increase JSONL lookback from 100 to 500 lines to handle long tool execution progress streaks
- Never show "idle" for sessions with active processes

## [0.1.22] - 2026-02-06

### Fixed
- Use 30-second activity window for tool execution status instead of 3 seconds
- Sessions running tools no longer flicker to "waiting" between progress writes

## [0.1.21] - 2026-02-05

### Fixed
- Remove CPU-based status override that falsely showed finished sessions as "processing"

## [0.1.20] - 2026-02-05

### Fixed
- Filter out orphaned sessions whose terminal was closed (processes reparented to launchd)
- Clean up stale status tracking entries to prevent unbounded memory growth

## [0.1.17] - 2025-12-07

### Fixed
- Session detection for paths with dashes in folder names (worktrees, subfolders)

## [0.1.16] - 2025-12-06

### Changed
- Version bump for release

## [0.1.15] - 2025-12-06

### Added
- "Kill Session" option in session card menu to terminate Claude Code processes

### Changed
- Default hotkey changed from Option+Space to Control+Space
- Git branch now shows proper branch icon instead of lightning bolt
- Dev server port changed from 1420 to 1422 to avoid conflicts

## [0.1.14] - 2025-12-06

### Fixed
- Improved status detection to prevent premature transition to "Waiting" while Claude is still streaming
- Added stable session ordering in UI to prevent unnecessary reordering on each poll
- Enhanced debug logging with status transition tracking and content previews

## [0.1.13] - 2025-12-06

### Added
- Sub-agent count badge `[+N]` displayed on sessions with active sub-agents
- `activeSubagentCount` field to Session model

### Fixed
- Filter out sub-agent processes (parent is another Claude process)
- Filter out Zed external agents (claude-code-acp) that aren't user-initiated
- Exclude `agent-*.jsonl` files from main session detection to prevent duplicates

## [0.1.12] - 2025-12-05

### Changed
- Reduced poll interval to 2 seconds for faster updates

## [0.1.11] - 2025-12-05

### Added
- "Open GitHub" menu item to open project's GitHub repo
