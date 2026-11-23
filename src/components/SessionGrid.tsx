import { Session } from '../types/session';
import { SessionCard } from './SessionCard';

interface SessionGridProps {
  sessions: Session[];
  onSessionClick: (session: Session) => void;
  onSessionUnpin: (session: Session) => void | Promise<void>;
}

export function SessionGrid({ sessions, onSessionClick, onSessionUnpin }: SessionGridProps) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
      {sessions.map((session) => (
        <SessionCard
          key={`${session.agentType}-${session.id}-${session.pid}`}
          session={session}
          onClick={() => onSessionClick(session)}
          onUnpin={() => onSessionUnpin(session)}
        />
      ))}
    </div>
  );
}
