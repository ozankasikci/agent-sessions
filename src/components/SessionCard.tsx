import { Session } from '../types/session';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';

interface SessionCardProps {
  session: Session;
  onClick: () => void;
}

const statusConfig = {
  waiting: {
    color: 'bg-status-waiting',
    variant: 'outline' as const,
    className: 'border-status-waiting/30 text-status-waiting bg-status-waiting/10',
    label: 'Waiting for input',
  },
  thinking: {
    color: 'bg-status-thinking',
    variant: 'outline' as const,
    className: 'border-status-thinking/30 text-status-thinking bg-status-thinking/10',
    label: 'Thinking...',
  },
  processing: {
    color: 'bg-status-processing',
    variant: 'outline' as const,
    className: 'border-status-processing/30 text-status-processing bg-status-processing/10',
    label: 'Processing',
  },
  idle: {
    color: 'bg-status-idle',
    variant: 'outline' as const,
    className: 'border-status-idle/30 text-status-idle bg-status-idle/10',
    label: 'Idle',
  },
};

function formatTimeAgo(timestamp: string): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;

  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;

  const diffDays = Math.floor(diffHours / 24);
  return `${diffDays}d ago`;
}

function truncatePath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, '~');
}

export function SessionCard({ session, onClick }: SessionCardProps) {
  const config = statusConfig[session.status];

  return (
    <Card
      className="group cursor-pointer transition-all duration-200 hover:bg-accent/50 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 py-0 gap-0"
      onClick={onClick}
    >
      <CardContent className="p-4">
        {/* Header: Project name + Status indicator */}
        <div className="flex items-start justify-between gap-3 mb-3">
          <div className="flex-1 min-w-0">
            <h3 className="font-semibold text-base text-foreground truncate group-hover:text-primary transition-colors">
              {session.projectName}
            </h3>
            <p className="text-xs text-muted-foreground truncate mt-0.5">
              {truncatePath(session.projectPath)}
            </p>
          </div>
          <div className="flex items-center gap-2 shrink-0">
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
        {session.lastMessage && (
          <div className="text-sm text-muted-foreground mb-3 line-clamp-2 leading-relaxed">
            {session.lastMessage}
          </div>
        )}

        {/* Footer: Status Badge + Time */}
        <div className="flex items-center justify-between pt-3 border-t border-border">
          <Badge variant="outline" className={config.className}>
            {config.label}
          </Badge>
          <span className="text-xs text-muted-foreground">
            {formatTimeAgo(session.lastActivityAt)}
          </span>
        </div>
      </CardContent>
    </Card>
  );
}
