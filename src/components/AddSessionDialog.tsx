import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { formatTimeAgo, truncatePath } from '@/lib/formatters';
import { ManualSessionCandidate } from '@/types/session';

interface AddSessionDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onSessionAdded: () => void | Promise<void>;
}

const sourceLabels = {
  claude: 'Claude',
  codex: 'Codex',
  opencode: 'OpenCode',
  antigravity: 'Antigravity',
} as const;

export function AddSessionDialog({
  isOpen,
  onClose,
  onSessionAdded,
}: AddSessionDialogProps) {
  const [candidates, setCandidates] = useState<ManualSessionCandidate[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [addingPath, setAddingPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadCandidates = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<ManualSessionCandidate[]>('list_manual_session_candidates');
      setCandidates(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      void loadCandidates();
    }
  }, [isOpen, loadCandidates]);

  const handleAdd = async (candidate: ManualSessionCandidate) => {
    setAddingPath(candidate.filePath);
    setError(null);
    try {
      await invoke('add_manual_session', {
        filePath: candidate.filePath,
        agentType: candidate.agentType,
      });
      setCandidates((current) => current.filter((item) => item.filePath !== candidate.filePath));
      await onSessionAdded();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setAddingPath(null);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-2xl gap-4">
        <DialogHeader>
          <DialogTitle>Add session</DialogTitle>
          <DialogDescription>
            Pin a recent Claude Code or Codex session to keep it in the viewer.
          </DialogDescription>
        </DialogHeader>

        {error && (
          <div className="rounded-lg border border-destructive/20 bg-destructive/10 p-3 text-sm text-destructive">
            {error}
          </div>
        )}

        <div className="max-h-[60vh] space-y-2 overflow-y-auto pr-1">
          {isLoading ? (
            <div className="py-12 text-center text-sm text-muted-foreground">
              Loading recent sessions...
            </div>
          ) : candidates.length === 0 ? (
            <div className="py-12 text-center text-sm text-muted-foreground">
              No recent unpinned sessions found.
            </div>
          ) : (
            candidates.map((candidate) => (
              <div
                key={`${candidate.agentType}-${candidate.id}`}
                className="flex items-start gap-4 rounded-lg border border-border bg-card/60 p-3"
              >
                <div className="min-w-0 flex-1">
                  <div className="mb-1 flex items-center gap-2">
                    <h3 className="truncate text-sm font-medium text-foreground">
                      {candidate.projectName}
                    </h3>
                    <Badge variant="secondary" className="shrink-0 text-[10px]">
                      {sourceLabels[candidate.agentType]}
                    </Badge>
                    <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                      {formatTimeAgo(candidate.lastActivityAt)}
                    </span>
                  </div>
                  <p className="truncate text-xs text-muted-foreground">
                    {truncatePath(candidate.projectPath)}
                  </p>
                  {candidate.lastMessage && (
                    <p className="mt-2 line-clamp-2 text-xs leading-relaxed text-muted-foreground">
                      {candidate.lastMessage}
                    </p>
                  )}
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={addingPath !== null}
                  onClick={() => void handleAdd(candidate)}
                >
                  {addingPath === candidate.filePath ? 'Adding...' : 'Add'}
                </Button>
              </div>
            ))
          )}
        </div>

        {!isLoading && (
          <div className="flex justify-between border-t border-border pt-3">
            <p className="text-xs text-muted-foreground">
              Showing up to 50 recent sessions per source
            </p>
            <Button variant="ghost" size="sm" onClick={() => void loadCandidates()}>
              Refresh
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
