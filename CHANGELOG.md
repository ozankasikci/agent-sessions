# Changelog

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
