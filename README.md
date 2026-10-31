# Agent Sessions

[![GitHub release](https://img.shields.io/github/v/release/ozankasikci/agent-sessions)](https://github.com/ozankasikci/agent-sessions/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![macOS](https://img.shields.io/badge/macOS-Monterey%2B-black)](https://github.com/ozankasikci/agent-sessions/releases)
[![Homebrew](https://img.shields.io/badge/Homebrew-available-orange)](https://github.com/ozankasikci/homebrew-tap)

A macOS desktop app to monitor your AI coding agents in real-time.

![Demo](demo/claude-sessions-demo.gif)

## Supported Agents

| Agent | Terminal (CLI) | Desktop app | IDE extension |
|-------|:---:|:---:|:---:|
| **Claude Code** | ✅ `claude` | ✅ Claude app | ✅ |
| **Codex** | ✅ `codex` | ✅ Codex app | ✅ |
| **Antigravity** | ✅ `agy` | ⚠️ best effort | — |
| **OpenCode** | ✅ `opencode` | — | — |

Sessions are read locally from each agent's own logs (nothing is sent anywhere):

- Claude Code: `~/.claude/projects/`
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` (or `$CODEX_HOME`)
- Antigravity: `~/.gemini/antigravity-cli/brain/` (or `$ANTIGRAVITY_HOME`) and `~/.gemini/antigravity/brain/` for the app
- OpenCode: `~/.local/share/opencode/storage/`

Terminal sessions are matched to their running process by working directory. Desktop apps host many conversations in one process, so while an app is running, its conversations active in the last 30 minutes are shown. The Antigravity app's own conversation store is an undocumented binary format; its sessions are inferred from the files it writes under `brain/`.

## Features

- View all active coding agent sessions in one place
- Real-time status detection (Thinking, Processing, Waiting, Idle)
- Global hotkey to toggle visibility (default: `Ctrl+Space`, configurable)
- Click to focus on a specific session's terminal
- Custom session names (rename via kebab menu)
- Quick access URL for each session (e.g., dev server links)

> **Note:** Currently supports macOS only with iTerm2 and Terminal. Support for other terminals coming soon.

## Installation

### Homebrew (recommended)

```bash
brew tap ozankasikci/tap
brew install --cask agent-sessions
```

### DMG

Download the latest DMG from [Releases](https://github.com/ozankasikci/agent-sessions/releases).

## Tech Stack

- Tauri 2.x
- React + TypeScript
- Tailwind CSS + shadcn/ui
