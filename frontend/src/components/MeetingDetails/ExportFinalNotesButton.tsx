"use client";

import { useEffect, useState } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { FileDown, Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import Analytics from '@/lib/analytics';
import { exportFinalNotes, fileNameOf, openFinalNotesFolder } from '@/lib/finalNotes';

interface ExportFinalNotesButtonProps {
  meetingId: string;
  disabled?: boolean;
  /** Save unsaved edits first so the file matches what is on screen. */
  beforeExport?: () => Promise<void>;
}

/**
 * Fork: write this meeting to the FinalNotes folder now. Automatic exports
 * run in the background; their failures are reported here as well.
 */
export function ExportFinalNotesButton({ meetingId, disabled, beforeExport }: ExportFinalNotesButtonProps) {
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    const unlisten: Promise<UnlistenFn> = listen<{ meeting_id: string; error: string }>(
      'final-notes-export-error',
      (e) => {
        if (e.payload.meeting_id === meetingId) toast.error(e.payload.error);
      },
    );
    return () => {
      unlisten.then((un) => un());
    };
  }, [meetingId]);

  const handleExport = async () => {
    setExporting(true);
    try {
      if (beforeExport) await beforeExport();
      const path = await exportFinalNotes(meetingId);
      toast.success('Exported to FinalNotes', {
        description: fileNameOf(path),
        action: { label: 'Open folder', onClick: () => void openFinalNotesFolder().catch((err) => toast.error(String(err))) },
      });
    } catch (err) {
      toast.error(String(err));
    } finally {
      setExporting(false);
    }
  };

  return (
    <Button
      variant="outline"
      size="sm"
      title="Save this meeting as a Markdown file in the FinalNotes folder"
      disabled={disabled || exporting}
      onClick={() => {
        Analytics.trackButtonClick('export_final_notes', 'meeting_details');
        void handleExport();
      }}
    >
      {exporting ? <Loader2 className="animate-spin" /> : <FileDown />}
      <span className="hidden @[40rem]:inline">Export</span>
    </Button>
  );
}
