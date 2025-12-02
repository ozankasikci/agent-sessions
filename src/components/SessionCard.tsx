import { Session } from '../types/session';

interface SessionCardProps {
  session: Session;
  onClick: () => void;
}

const statusConfig = {
  waiting: {
    color: 'bg-amber-400',
    bgColor: 'bg-amber-400/10',
    borderColor: 'border-amber-400/20',
    label: 'Waiting for input',
  },
  thinking: {
    color: 'bg-purple-400',
    bgColor: 'bg-purple-400/10',
    borderColor: 'border-purple-400/20',
    label: 'Thinking...',
  },
  processing: {
    color: 'bg-emerald-400',
    bgColor: 'bg-emerald-400/10',
    borderColor: 'border-emerald-400/20',
    label: 'Processing',
  },
  idle: {
    color: 'bg-white/30',
    bgColor: 'bg-white/5',
    borderColor: 'border-white/10',
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
  // Replace home dir with ~
  return path.replace(/^\/Users\/[^/]+/, '~');
}

export function SessionCard({ session, onClick }: SessionCardProps) {
  const config = statusConfig[session.status];

  return (
    <button
      onClick={onClick}
      className={`w-full text-left p-4 rounded-xl border transition-all duration-200 cursor-pointer group
        ${config.bgColor} ${config.borderColor} hover:border-white/20 hover:bg-white/10`}
    >
      {/* Header: Project name */}
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="flex-1 min-w-0">
          <h3 className="font-semibold text-base text-white truncate group-hover:text-white">
            {session.projectName}
          </h3>
          <p className="text-xs text-white/40 truncate mt-0.5">
            {truncatePath(session.projectPath)}
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span className={`w-2 h-2 rounded-full ${config.color}`} />
        </div>
      </div>

      {/* Git branch */}
      {session.gitBranch && (
        <div className="flex items-center gap-1.5 mb-3">
          <svg className="w-3.5 h-3.5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
          <span className="text-xs text-white/50 truncate">
            {session.gitBranch}
          </span>
        </div>
      )}

      {/* Message Preview */}
      {session.lastMessage && (
        <div className="text-sm text-white/60 mb-3 line-clamp-2 leading-relaxed">
          {session.lastMessage}
        </div>
      )}

      {/* Footer: Status + Time */}
      <div className="flex items-center justify-between text-xs pt-2 border-t border-white/5">
        <span className="text-white/40">
          {config.label}
        </span>
        <span className="text-white/30">
          {formatTimeAgo(session.lastActivityAt)}
        </span>
      </div>
    </button>
  );
}
