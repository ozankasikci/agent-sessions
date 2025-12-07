# Changelog

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
