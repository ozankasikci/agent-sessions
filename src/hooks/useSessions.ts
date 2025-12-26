import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Session, SessionsResponse } from '../types/session';

const POLL_INTERVAL = 2000; // 2 seconds

// Get ordering priority for card stability (only distinguishes active vs idle)
// This prevents card reordering when status flips between thinking/processing/waiting
function getOrderingPriority(status: string): number {
  switch (status) {
    case 'thinking':
    case 'processing':
    case 'waiting':
      return 0; // All active states - same ordering priority
    case 'idle':
      return 1; // Only idle causes reordering
    default:
      return 2;
  }
}

// Merge new sessions with existing order, only reordering when priority changes
function mergeWithStableOrder(existing: Session[], incoming: Session[]): Session[] {
  if (existing.length === 0) {
    return incoming;
  }

  // Create a map of existing positions by session ID
  const existingOrder = new Map<string, number>();
  existing.forEach((s, idx) => existingOrder.set(s.id, idx));

  // Create a map of existing ordering priorities (coarse: active vs idle)
  const existingPriority = new Map<string, number>();
  existing.forEach(s => existingPriority.set(s.id, getOrderingPriority(s.status)));

  // Check if any session changed ordering tier (only active <-> idle triggers reorder)
  // Status changes within active states (thinking/processing/waiting) don't cause reordering
  let priorityChanged = false;
  for (const session of incoming) {
    const oldPriority = existingPriority.get(session.id);
    const newPriority = getOrderingPriority(session.status);
    if (oldPriority !== undefined && oldPriority !== newPriority) {
      priorityChanged = true;
      break;
    }
  }

  // Also check for new sessions
  const hasNewSessions = incoming.some(s => !existingOrder.has(s.id));

  // If priority changed or new sessions appeared, use backend order
  if (priorityChanged || hasNewSessions) {
    return incoming;
  }

  // Otherwise, preserve existing order but update session data
  const incomingMap = new Map<string, Session>();
  incoming.forEach(s => incomingMap.set(s.id, s));

  // Keep existing order, update with new data
  const result: Session[] = [];
  for (const existingSession of existing) {
    const updated = incomingMap.get(existingSession.id);
    if (updated) {
      result.push(updated);
      incomingMap.delete(existingSession.id);
    }
  }

  // Add any remaining new sessions at the end (shouldn't happen if hasNewSessions check works)
  for (const newSession of incomingMap.values()) {
    result.push(newSession);
  }

  return result;
}

export function useSessions() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [waitingCount, setWaitingCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const sessionsRef = useRef<Session[]>([]);

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
      // Merge with stable ordering to prevent unnecessary reordering
      const stableSessions = mergeWithStableOrder(sessionsRef.current, response.sessions);
      sessionsRef.current = stableSessions;
      setSessions([...stableSessions]);
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
