import { useState, useEffect } from 'react';
import { Session } from '../types/session';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { formatTimeAgo, truncatePath, statusConfig } from '@/lib/formatters';
import { openUrl } from '@tauri-apps/plugin-opener';

interface SessionCardProps {
  session: Session;
  onClick: () => void;
}

// Helper to get/set custom data from localStorage
const CUSTOM_NAMES_KEY = 'agent-sessions-custom-names';
const CUSTOM_URLS_KEY = 'agent-sessions-custom-urls';

function getCustomNames(): Record<string, string> {
  try {
    const stored = localStorage.getItem(CUSTOM_NAMES_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function setCustomName(sessionId: string, name: string) {
  const names = getCustomNames();
  if (name.trim()) {
    names[sessionId] = name.trim();
  } else {
    delete names[sessionId];
  }
  localStorage.setItem(CUSTOM_NAMES_KEY, JSON.stringify(names));
}

function getCustomUrls(): Record<string, string> {
  try {
    const stored = localStorage.getItem(CUSTOM_URLS_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function setCustomUrl(sessionId: string, url: string) {
  const urls = getCustomUrls();
  if (url.trim()) {
    urls[sessionId] = url.trim();
  } else {
    delete urls[sessionId];
  }
  localStorage.setItem(CUSTOM_URLS_KEY, JSON.stringify(urls));
}

export function SessionCard({ session, onClick }: SessionCardProps) {
  const config = statusConfig[session.status];
  const [customName, setCustomNameState] = useState<string>('');
  const [customUrl, setCustomUrlState] = useState<string>('');
  const [isRenameOpen, setIsRenameOpen] = useState(false);
  const [isUrlOpen, setIsUrlOpen] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [urlValue, setUrlValue] = useState('');

  // Load custom data on mount
  useEffect(() => {
    const names = getCustomNames();
    const urls = getCustomUrls();
    setCustomNameState(names[session.id] || '');
    setCustomUrlState(urls[session.id] || '');
  }, [session.id]);

  const displayName = customName || session.projectName;

  const handleRename = () => {
    setRenameValue(customName || session.projectName);
    setIsRenameOpen(true);
  };

  const handleSaveRename = () => {
    const newName = renameValue.trim();
    if (newName === session.projectName) {
      setCustomName(session.id, '');
      setCustomNameState('');
    } else {
      setCustomName(session.id, newName);
      setCustomNameState(newName);
    }
    setIsRenameOpen(false);
  };

  const handleResetName = () => {
    setCustomName(session.id, '');
    setCustomNameState('');
    setIsRenameOpen(false);
  };

  const handleSetUrl = () => {
    setUrlValue(customUrl);
    setIsUrlOpen(true);
  };

  const handleSaveUrl = () => {
    const newUrl = urlValue.trim();
    setCustomUrl(session.id, newUrl);
    setCustomUrlState(newUrl);
    setIsUrlOpen(false);
  };

  const handleClearUrl = () => {
    setCustomUrl(session.id, '');
    setCustomUrlState('');
    setIsUrlOpen(false);
  };

  const handleOpenUrl = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (customUrl) {
      // Add protocol if missing
      let url = customUrl;
      if (!url.startsWith('http://') && !url.startsWith('https://')) {
        url = 'http://' + url;
      }
      await openUrl(url);
    }
  };

  const handleOpenGitHub = async () => {
    if (session.githubUrl) {
      await openUrl(session.githubUrl);
    }
  };

  return (
    <>
      <Card
        className={`group cursor-pointer transition-all duration-200 hover:shadow-lg py-0 gap-0 h-full flex flex-col ${config.cardBg} ${config.cardBorder} hover:border-primary/30`}
        onClick={onClick}
      >
        <CardContent className="p-4 flex flex-col flex-1">
          {/* Header: Project name + Menu + Status indicator */}
          <div className="flex items-start justify-between gap-2 mb-3">
            <div className="flex-1 min-w-0">
              <h3 className="font-semibold text-base text-foreground truncate group-hover:text-primary transition-colors">
                {displayName}
              </h3>
              <p className="text-xs text-muted-foreground truncate mt-0.5">
                {truncatePath(session.projectPath)}
              </p>
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              {/* URL Button - visible on hover if URL is set */}
              {customUrl && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100 transition-opacity hover:bg-primary/10"
                  onClick={handleOpenUrl}
                  title={customUrl}
                >
                  <svg
                    className="w-4 h-4 text-muted-foreground"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
                    />
                  </svg>
                </Button>
              )}
              <DropdownMenu>
                <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    <svg
                      className="w-4 h-4 text-muted-foreground"
                      fill="currentColor"
                      viewBox="0 0 20 20"
                    >
                      <path d="M10 6a2 2 0 110-4 2 2 0 010 4zM10 12a2 2 0 110-4 2 2 0 010 4zM10 18a2 2 0 110-4 2 2 0 010 4z" />
                    </svg>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
                  <DropdownMenuItem onClick={handleRename}>
                    <svg
                      className="w-4 h-4 mr-2"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                      />
                    </svg>
                    Rename
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={handleSetUrl}>
                    <svg
                      className="w-4 h-4 mr-2"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
                      />
                    </svg>
                    {customUrl ? 'Edit URL' : 'Set URL'}
                  </DropdownMenuItem>
                  {session.githubUrl && (
                    <DropdownMenuItem onClick={handleOpenGitHub}>
                      <svg
                        className="w-4 h-4 mr-2"
                        fill="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
                      </svg>
                      Open GitHub
                    </DropdownMenuItem>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
              <span className={`w-2.5 h-2.5 rounded-full ${config.color} shadow-sm shadow-current`} />
            </div>
          </div>

          {/* Git branch */}
          {session.gitBranch && (
            <div className="flex items-center gap-1.5 mb-3">
              <svg className="w-3.5 h-3.5 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              <span className="text-xs text-muted-foreground truncate">
                {session.gitBranch}
              </span>
            </div>
          )}

          {/* Message Preview */}
          <div className="flex-1">
            {session.lastMessage && (
              <div className="text-sm text-muted-foreground line-clamp-2 leading-relaxed">
                {session.lastMessage}
              </div>
            )}
          </div>

          {/* Footer: Status Badge + Time */}
          <div className="flex items-center justify-between pt-3 mt-3 border-t border-border">
            <div className="flex items-center gap-2">
              <Badge variant="outline" className={config.badgeClassName}>
                {config.label}
              </Badge>
              {session.activeSubagentCount > 0 && (
                <span className="text-xs text-muted-foreground">
                  [+{session.activeSubagentCount}]
                </span>
              )}
            </div>
            <span className="text-xs text-muted-foreground">
              {formatTimeAgo(session.lastActivityAt)}
            </span>
          </div>
        </CardContent>
      </Card>

      {/* Rename Dialog */}
      <Dialog open={isRenameOpen} onOpenChange={setIsRenameOpen}>
        <DialogContent onClick={(e) => e.stopPropagation()}>
          <DialogHeader>
            <DialogTitle>Rename Session</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              placeholder="Enter custom name"
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  handleSaveRename();
                }
              }}
              autoFocus
            />
            <p className="text-xs text-muted-foreground mt-2">
              Original: {session.projectName}
            </p>
          </div>
          <DialogFooter className="flex gap-2">
            {customName && (
              <Button variant="outline" onClick={handleResetName}>
                Reset to Original
              </Button>
            )}
            <Button variant="outline" onClick={() => setIsRenameOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleSaveRename}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* URL Dialog */}
      <Dialog open={isUrlOpen} onOpenChange={setIsUrlOpen}>
        <DialogContent onClick={(e) => e.stopPropagation()}>
          <DialogHeader>
            <DialogTitle>Set Development URL</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <Input
              value={urlValue}
              onChange={(e) => setUrlValue(e.target.value)}
              placeholder="e.g., localhost:3000"
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  handleSaveUrl();
                }
              }}
              autoFocus
            />
            <p className="text-xs text-muted-foreground mt-2">
              Quick access URL for this project (e.g., dev server)
            </p>
          </div>
          <DialogFooter className="flex gap-2">
            {customUrl && (
              <Button variant="outline" onClick={handleClearUrl}>
                Clear URL
              </Button>
            )}
            <Button variant="outline" onClick={() => setIsUrlOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleSaveUrl}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
