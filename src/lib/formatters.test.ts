import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { formatTimeAgo, truncatePath, statusConfig } from './formatters';

describe('formatTimeAgo', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns "just now" for timestamps less than a minute ago', () => {
    const now = new Date('2024-01-15T12:00:00Z');
    vi.setSystemTime(now);

    const timestamp = new Date('2024-01-15T11:59:30Z').toISOString();
    expect(formatTimeAgo(timestamp)).toBe('just now');
  });

  it('returns minutes ago for timestamps less than an hour ago', () => {
    const now = new Date('2024-01-15T12:00:00Z');
    vi.setSystemTime(now);

    const timestamp = new Date('2024-01-15T11:45:00Z').toISOString();
    expect(formatTimeAgo(timestamp)).toBe('15m ago');
  });

  it('returns hours ago for timestamps less than a day ago', () => {
    const now = new Date('2024-01-15T12:00:00Z');
    vi.setSystemTime(now);

    const timestamp = new Date('2024-01-15T09:00:00Z').toISOString();
    expect(formatTimeAgo(timestamp)).toBe('3h ago');
  });

  it('returns days ago for timestamps more than a day ago', () => {
    const now = new Date('2024-01-15T12:00:00Z');
    vi.setSystemTime(now);

    const timestamp = new Date('2024-01-12T12:00:00Z').toISOString();
    expect(formatTimeAgo(timestamp)).toBe('3d ago');
  });
});

describe('truncatePath', () => {
  it('replaces /Users/<username> with ~', () => {
    expect(truncatePath('/Users/john/Projects/my-app')).toBe('~/Projects/my-app');
    expect(truncatePath('/Users/ozan/Documents/code')).toBe('~/Documents/code');
  });

  it('preserves paths that do not start with /Users', () => {
    expect(truncatePath('/var/log/app.log')).toBe('/var/log/app.log');
    expect(truncatePath('/tmp/project')).toBe('/tmp/project');
  });

  it('handles paths with hyphenated usernames', () => {
    expect(truncatePath('/Users/john-doe/Projects/app')).toBe('~/Projects/app');
  });
});

describe('statusConfig', () => {
  it('has configuration for all session statuses', () => {
    expect(statusConfig.waiting).toBeDefined();
    expect(statusConfig.thinking).toBeDefined();
    expect(statusConfig.processing).toBeDefined();
    expect(statusConfig.idle).toBeDefined();
  });

  it('each status has required properties', () => {
    const requiredProperties = ['color', 'cardBg', 'cardBorder', 'badgeClassName', 'label'];

    for (const status of Object.values(statusConfig)) {
      for (const prop of requiredProperties) {
        expect(status).toHaveProperty(prop);
        expect(typeof status[prop as keyof typeof status]).toBe('string');
      }
    }
  });

  it('has human-readable labels', () => {
    expect(statusConfig.waiting.label).toBe('Waiting for input');
    expect(statusConfig.thinking.label).toBe('Thinking...');
    expect(statusConfig.processing.label).toBe('Processing');
    expect(statusConfig.idle.label).toBe('Idle');
  });
});
