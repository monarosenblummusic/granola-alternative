"use client";

import { useEffect, useState } from 'react';
import { FolderOpen, FolderInput } from 'lucide-react';
import { toast } from 'sonner';
import { Switch } from './ui/switch';
import {
  chooseFinalNotesFolder,
  FinalNotesSettings as Settings,
  getFinalNotesSettings,
  openFinalNotesFolder,
  saveFinalNotesSettings,
} from '@/lib/finalNotes';

/** Fork: where meeting notes are exported for Claude, and whether it happens automatically. */
export function FinalNotesSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    getFinalNotesSettings()
      .then(setSettings)
      .catch((err) => toast.error(`Could not load FinalNotes settings: ${err}`));
  }, []);

  const update = async (next: Settings) => {
    try {
      setSettings(await saveFinalNotesSettings(next));
    } catch (err) {
      toast.error(String(err));
    }
  };

  const changeFolder = async () => {
    if (!settings) return;
    try {
      const folder = await chooseFinalNotesFolder();
      if (folder) await update({ ...settings, folder });
    } catch (err) {
      toast.error(String(err));
    }
  };

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h3 className="text-lg font-semibold text-gray-900 mb-2">FinalNotes export</h3>
          <p className="text-sm text-gray-600">
            Save every meeting as a Markdown file (AI notes, your notes and the transcript) after its summary is ready,
            and keep the file up to date when you edit the meeting. Point Claude at this folder to ask about your meetings.
          </p>
        </div>
        <Switch
          checked={settings?.auto_export ?? false}
          disabled={!settings}
          onCheckedChange={(checked) => settings && void update({ ...settings, auto_export: checked })}
          aria-label="Export meetings automatically"
        />
      </div>

      <div className="mt-4 p-4 border rounded-lg bg-gray-50">
        <div className="font-medium mb-2">Folder</div>
        <div className="text-sm text-gray-600 mb-3 break-all font-mono text-xs">{settings?.folder ?? 'Loading...'}</div>
        <div className="flex gap-2">
          <button
            onClick={() => void changeFolder()}
            disabled={!settings}
            className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
          >
            <FolderInput className="w-4 h-4" />
            Change Folder
          </button>
          <button
            onClick={() => openFinalNotesFolder().catch((err) => toast.error(String(err)))}
            className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
          >
            <FolderOpen className="w-4 h-4" />
            Open Folder
          </button>
        </div>
      </div>
    </div>
  );
}
