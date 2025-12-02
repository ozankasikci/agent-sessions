# Claude Sessions Viewer

A macOS desktop app to monitor all running Claude Code sessions.

https://github.com/user-attachments/assets/placeholder

## Features

- View all active Claude Code sessions in one place
- Real-time status detection (Thinking, Processing, Waiting, Idle)
- Global hotkey to toggle visibility (configurable)
- Click to focus on a specific session's terminal

## Installation

```bash
npm install
npm run tauri build
```

The built app will be at `src-tauri/target/release/bundle/dmg/`.

## Development

```bash
npm run tauri dev
```

## Tech Stack

- Tauri 2.x
- React + TypeScript
- Tailwind CSS + shadcn/ui
