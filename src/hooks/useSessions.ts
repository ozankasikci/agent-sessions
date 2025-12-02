import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Session, SessionsResponse } from '../types/session';

const POLL_INTERVAL = 3000; // 3 seconds

export function useSessions() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [waitingCount, setWaitingCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const updateTrayTitle = useCallback(async (total: number, waiting: number) => {
    try {
      await invoke('update_tray_title', { total, waiting });
    } catch (err) {
      console.error('Failed to update tray title:', err);
    }
  }, []);

  const fetchSessions = useCallback(async () => {
    try {
      const response = await invoke<SessionsResponse>('get_all_sessions');
      // Always create a new array reference to ensure React detects changes
      setSessions([...response.sessions]);
      setTotalCount(response.totalCount);
      setWaitingCount(response.waitingCount);
      setError(null);

      // Update tray icon title with counts
      await updateTrayTitle(response.totalCount, response.waitingCount);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch sessions');
    } finally {
      setIsLoading(false);
    }
  }, [updateTrayTitle]);

  const focusSession = useCallback(async (session: Session) => {
    try {
      await invoke('focus_session', {
        pid: session.pid,
        projectPath: session.projectPath,
      });
    } catch (err) {
      console.error('Failed to focus session:', err);
    }
  }, []);

  // Initial fetch
  useEffect(() => {
    fetchSessions();
  }, [fetchSessions]);

  // Polling
  useEffect(() => {
    const interval = setInterval(fetchSessions, POLL_INTERVAL);
    return () => clearInterval(interval);
  }, [fetchSessions]);

  return {
    sessions,
    totalCount,
    waitingCount,
    isLoading,
    error,
    refresh: fetchSessions,
    focusSession,
  };
}
