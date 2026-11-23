import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AddSessionDialog } from '../components/AddSessionDialog';
import { SessionCard } from '../components/SessionCard';
import { ManualSessionCandidate, Session } from '../types/session';

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

const candidate: ManualSessionCandidate = {
  id: 'codex-session',
  agentType: 'codex',
  projectName: 'codex-project',
  projectPath: '/Users/test/Projects/codex-project',
  lastMessage: 'The latest Codex response',
  lastMessageRole: 'assistant',
  lastActivityAt: '2026-03-07T05:24:01.000Z',
  filePath: '/Users/test/.codex/sessions/rollout-test.jsonl',
};

const manualSession: Session = {
  id: 'manual-session',
  agentType: 'codex',
  projectName: 'manual-project',
  projectPath: '/Users/test/Projects/manual-project',
  gitBranch: null,
  githubUrl: null,
  status: 'waiting',
  lastMessage: 'Pinned response',
  lastMessageRole: 'assistant',
  lastActivityAt: '2026-03-07T05:24:01.000Z',
  pid: 0,
  cpuUsage: 0,
  activeSubagentCount: 0,
  client: 'cli',
  appName: null,
  sharedProcess: false,
  isManual: true,
};

describe('manual session UI', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('lists a candidate and pins it through the backend command', async () => {
    invokeMock
      .mockResolvedValueOnce([candidate] as never)
      .mockResolvedValueOnce(undefined as never);
    const onSessionAdded = vi.fn().mockResolvedValue(undefined);

    render(
      <AddSessionDialog
        isOpen
        onClose={vi.fn()}
        onSessionAdded={onSessionAdded}
      />,
    );

    expect(await screen.findByText('codex-project')).toBeInTheDocument();
    expect(screen.getByText('Codex')).toBeInTheDocument();
    expect(screen.getByText('The latest Codex response')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('add_manual_session', {
        filePath: candidate.filePath,
        agentType: 'codex',
      });
      expect(onSessionAdded).toHaveBeenCalledOnce();
    });
  });

  it('does not focus a pinned card and offers removal instead of kill', async () => {
    const onClick = vi.fn();
    const onUnpin = vi.fn();

    render(
      <SessionCard
        session={manualSession}
        onClick={onClick}
        onUnpin={onUnpin}
      />,
    );

    fireEvent.click(screen.getByText('manual-project'));
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.getByText('Pinned')).toBeInTheDocument();
    expect(screen.getByTitle('Codex')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByRole('button', { name: 'Session actions' }), { key: 'Enter' });
    expect(await screen.findByText('Remove from viewer')).toBeInTheDocument();
    expect(screen.queryByText('Kill Session')).not.toBeInTheDocument();

    fireEvent.click(screen.getByText('Remove from viewer'));
    expect(onUnpin).toHaveBeenCalledOnce();
  });
});
