import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface SettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

const STORAGE_KEY = 'claude-sessions-hotkey';
const DEFAULT_HOTKEY = 'Option+Space';

export function Settings({ isOpen, onClose }: SettingsProps) {
  const [hotkey, setHotkey] = useState(DEFAULT_HOTKEY);
  const [isRecording, setIsRecording] = useState(false);
  const [recordedKeys, setRecordedKeys] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Load saved hotkey on mount
  useEffect(() => {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      setHotkey(saved);
    }
  }, []);

  // Register hotkey with backend
  const registerHotkey = useCallback(async (shortcut: string) => {
    try {
      await invoke('register_shortcut', { shortcut });
      setError(null);
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return false;
    }
  }, []);

  // Handle key recording
  useEffect(() => {
    if (!isRecording) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const keys: string[] = [];

      if (e.metaKey) keys.push('Command');
      if (e.ctrlKey) keys.push('Control');
      if (e.altKey) keys.push('Option');
      if (e.shiftKey) keys.push('Shift');

      // Add the actual key if it's not a modifier
      const key = e.key;
      if (!['Meta', 'Control', 'Alt', 'Shift'].includes(key)) {
        // Convert key to proper format
        let formattedKey = key;
        if (key === ' ') formattedKey = 'Space';
        else if (key.length === 1) formattedKey = key.toUpperCase();
        else if (key.startsWith('Arrow')) formattedKey = key;
        else if (key.startsWith('F') && key.length <= 3) formattedKey = key; // F1-F12

        keys.push(formattedKey);
      }

      setRecordedKeys(keys);
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();

      if (recordedKeys.length > 0 && !['Meta', 'Control', 'Alt', 'Shift'].includes(e.key)) {
        // We have a complete shortcut
        const shortcut = recordedKeys.join('+');
        setHotkey(shortcut);
        setIsRecording(false);
        setRecordedKeys([]);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [isRecording, recordedKeys]);

  const handleSave = async () => {
    const success = await registerHotkey(hotkey);
    if (success) {
      localStorage.setItem(STORAGE_KEY, hotkey);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    }
  };

  const handleClear = async () => {
    try {
      await invoke('unregister_shortcut');
      setHotkey('');
      localStorage.removeItem(STORAGE_KEY);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-[#1a1a1a] rounded-xl border border-white/10 p-6 w-80 shadow-2xl"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-lg font-semibold text-white">Settings</h2>
          <button
            onClick={onClose}
            className="text-white/40 hover:text-white/80 transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="space-y-4">
          <div>
            <label className="block text-sm text-white/60 mb-2">
              Global Hotkey
            </label>
            <div
              className={`p-3 rounded-lg border text-center cursor-pointer transition-all ${
                isRecording
                  ? 'border-purple-500 bg-purple-500/10 text-purple-300'
                  : 'border-white/10 bg-white/5 text-white/80 hover:border-white/20'
              }`}
              onClick={() => setIsRecording(true)}
            >
              {isRecording ? (
                recordedKeys.length > 0 ? recordedKeys.join(' + ') : 'Press keys...'
              ) : (
                hotkey || 'Click to set hotkey'
              )}
            </div>
            <p className="text-xs text-white/30 mt-2">
              Click and press your desired key combination
            </p>
          </div>

          {error && (
            <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-sm">
              {error}
            </div>
          )}

          {saved && (
            <div className="p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-sm">
              Hotkey saved!
            </div>
          )}

          <div className="flex gap-2 pt-2">
            <button
              onClick={handleClear}
              className="flex-1 px-4 py-2 rounded-lg border border-white/10 text-white/60 hover:bg-white/5 transition-colors text-sm"
            >
              Clear
            </button>
            <button
              onClick={handleSave}
              className="flex-1 px-4 py-2 rounded-lg bg-white/10 text-white hover:bg-white/20 transition-colors text-sm font-medium"
            >
              Save
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function useHotkeyInit() {
  useEffect(() => {
    const savedHotkey = localStorage.getItem(STORAGE_KEY);
    if (savedHotkey) {
      invoke('register_shortcut', { shortcut: savedHotkey }).catch(console.error);
    }
  }, []);
}
