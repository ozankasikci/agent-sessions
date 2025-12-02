import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

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

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure your Claude Sessions preferences
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium text-foreground">
              Global Hotkey
            </label>
            <div
              className={`flex items-center justify-center h-12 rounded-lg border cursor-pointer transition-all ${
                isRecording
                  ? 'border-primary bg-primary/10 text-primary'
                  : 'border-border bg-secondary/50 text-foreground hover:border-primary/50'
              }`}
              onClick={() => setIsRecording(true)}
            >
              <span className="text-sm font-medium">
                {isRecording ? (
                  recordedKeys.length > 0 ? recordedKeys.join(' + ') : 'Press keys...'
                ) : (
                  hotkey || 'Click to set hotkey'
                )}
              </span>
            </div>
            <p className="text-xs text-muted-foreground">
              Click and press your desired key combination
            </p>
          </div>

          {error && (
            <div className="p-3 rounded-lg bg-destructive/10 border border-destructive/20 text-destructive text-sm">
              {error}
            </div>
          )}

          {saved && (
            <div className="p-3 rounded-lg bg-status-processing/10 border border-status-processing/20 text-status-processing text-sm">
              Hotkey saved!
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={handleClear}>
            Clear
          </Button>
          <Button onClick={handleSave}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
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
