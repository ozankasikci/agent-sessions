export type SessionStatus = 'waiting' | 'processing' | 'thinking' | 'compacting' | 'idle';

export type AgentType = 'claude' | 'opencode' | 'codex' | 'antigravity';

/** Where a session runs: a terminal CLI, a desktop app, or an IDE extension */
export type SessionClient = 'cli' | 'desktop' | 'ide';

export interface Session {
  id: string;
  agentType: AgentType;
  projectName: string;
  projectPath: string;
  gitBranch: string | null;
  githubUrl: string | null;
  status: SessionStatus;
  lastMessage: string | null;
  lastMessageRole: 'user' | 'assistant' | null;
  lastActivityAt: string;
  pid: number;
  cpuUsage: number;
  activeSubagentCount: number;
  client: SessionClient;
  /** App to bring to the front when focusing a session without a terminal */
  appName: string | null;
  /** One process hosts many sessions (e.g. Codex app-server); killing it ends them all */
  sharedProcess: boolean;
}

export interface SessionsResponse {
  sessions: Session[];
  totalCount: number;
  waitingCount: number;
}
