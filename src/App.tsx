import { useState } from 'react';
import { SessionGrid } from './components/SessionGrid';
import { Settings, useHotkeyInit } from './components/Settings';
import { useSessions } from './hooks/useSessions';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';

function App() {
  const [showSettings, setShowSettings] = useState(false);
  const {
    sessions,
    totalCount,
    waitingCount,
    isLoading,
    error,
    refresh,
    focusSession,
  } = useSessions();

  // Initialize hotkey on app start
  useHotkeyInit();

  return (
    <div className="min-h-screen bg-background flex flex-col">
      {/* Draggable title bar area */}
      <header
        data-tauri-drag-region
        className="h-14 flex items-center justify-between px-6 border-b border-border bg-card/50 backdrop-blur-sm"
      >
        <div data-tauri-drag-region className="flex items-center gap-4 pl-16">
          <h1 data-tauri-drag-region className="text-lg font-semibold text-foreground">Claude Sessions</h1>
          {totalCount > 0 && (
            <div data-tauri-drag-region className="flex items-center gap-2">
              <Badge data-tauri-drag-region variant="secondary" className="font-medium pointer-events-none">
                {totalCount} active
              </Badge>
              {waitingCount > 0 && (
                <Badge data-tauri-drag-region className="bg-status-waiting/20 text-status-waiting border-status-waiting/30 font-medium pointer-events-none">
                  {waitingCount} waiting
                </Badge>
              )}
            </div>
          )}
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => setShowSettings(true)}
            title="Settings"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={refresh}
            disabled={isLoading}
            title="Refresh"
          >
            <svg
              className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
          </Button>
        </div>
      </header>

      {/* Settings Modal */}
      <Settings isOpen={showSettings} onClose={() => setShowSettings(false)} />

      {/* Main content area */}
      <main className="flex-1 overflow-y-auto p-6">
        {error ? (
          <div className="flex items-center justify-center h-full">
            <div className="p-6 text-destructive text-sm text-center bg-destructive/10 rounded-xl border border-destructive/20 max-w-md">
              <svg className="w-8 h-8 mx-auto mb-3 opacity-50" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
              </svg>
              {error}
            </div>
          </div>
        ) : sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="w-20 h-20 mb-6 rounded-2xl bg-muted/50 flex items-center justify-center border border-border">
              <svg className="w-10 h-10 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
              </svg>
            </div>
            <h2 className="text-lg font-medium text-foreground mb-2">No active sessions</h2>
            <p className="text-muted-foreground text-sm max-w-xs">
              Start a Claude session in your terminal to see it here
            </p>
          </div>
        ) : (
          <SessionGrid
            sessions={sessions}
            onSessionClick={focusSession}
          />
        )}
      </main>
    </div>
  );
}

export default App;
