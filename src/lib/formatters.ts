export function formatTimeAgo(timestamp: string): string {
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

export function truncatePath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, '~');
}

export const statusConfig = {
  waiting: {
    color: 'bg-white/50',
    cardBg: 'bg-white/5',
    cardBorder: 'border-white/10',
    badgeClassName: 'border-white/20 text-white/60 bg-white/5',
    label: 'Waiting for input',
  },
  thinking: {
    color: 'bg-purple-400',
    cardBg: 'bg-purple-400/15',
    cardBorder: 'border-purple-400/30',
    badgeClassName: 'border-purple-400/40 text-purple-300 bg-purple-400/20',
    label: 'Thinking...',
  },
  processing: {
    color: 'bg-emerald-400',
    cardBg: 'bg-emerald-400/15',
    cardBorder: 'border-emerald-400/30',
    badgeClassName: 'border-emerald-400/40 text-emerald-300 bg-emerald-400/20',
    label: 'Processing',
  },
  idle: {
    color: 'bg-white/30',
    cardBg: 'bg-white/5',
    cardBorder: 'border-white/10',
    badgeClassName: 'border-white/20 text-white/50 bg-white/5',
    label: 'Idle',
  },
} as const;
