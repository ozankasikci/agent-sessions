# Agent Sessions

[![GitHub release](https://img.shields.io/github/v/release/ozankasikci/agent-sessions)](https://github.com/ozankasikci/agent-sessions/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![macOS](https://img.shields.io/badge/macOS-Monterey%2B-black)](https://github.com/ozankasikci/agent-sessions/releases)
[![Homebrew](https://img.shields.io/badge/Homebrew-available-orange)](https://github.com/ozankasikci/homebrew-tap)

A macOS desktop app to monitor all running Claude Code sessions.

![Demo](demo/claude-sessions-demo.gif)

## Features

- View all active Claude Code sessions in one place
- Real-time status detection (Thinking, Processing, Waiting, Idle)
- Global hotkey to toggle visibility (configurable)
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
