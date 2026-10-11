"use client";

import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, FolderOpen, RefreshCw, Users } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { RetranscribeDialog } from './RetranscribeDialog';
import { useConfig } from '@/contexts/ConfigContext';


interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}


export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptButtonGroupProps) {
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);

  // Fork: identify individual speakers with Deepgram. It runs automatically
  // after a Deepgram recording (see lib/speakerIdentification.ts) or from the
  // Speakers button; either way this view listens for the result.
  const [identifying, setIdentifying] = useState(false);
  const refetchRef = useRef(onRefetchTranscripts);
  refetchRef.current = onRefetchTranscripts;

  useEffect(() => {
    if (!meetingId) return;
    const unlisteners: Promise<UnlistenFn>[] = [
      listen<{ meeting_id: string }>('speaker-identification-progress', (e) => {
        if (e.payload.meeting_id === meetingId) setIdentifying(true);
      }),
      listen<{ meeting_id: string; speakers: number }>('speaker-identification-complete', async (e) => {
        if (e.payload.meeting_id !== meetingId) return;
        setIdentifying(false);
        toast.success(`Found ${e.payload.speakers} other speaker${e.payload.speakers === 1 ? '' : 's'}`);
        if (refetchRef.current) await refetchRef.current();
      }),
      listen<{ meeting_id: string; error: string }>('speaker-identification-error', (e) => {
        if (e.payload.meeting_id !== meetingId) return;
        setIdentifying(false);
        toast.error(e.payload.error);
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, [meetingId]);

  const handleIdentifySpeakers = useCallback(async () => {
    if (!meetingId || !meetingFolderPath || identifying) return;
    setIdentifying(true);
    try {
      await invoke('identify_speakers_command', { meetingId, meetingFolderPath });
    } catch (err) {
      setIdentifying(false);
      toast.error(String(err));
    }
  }, [meetingId, meetingFolderPath, identifying]);

  const handleRetranscribeComplete = useCallback(async () => {
    // Refetch transcripts to show the updated data
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          className="px-2 @[22rem]:px-3"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden @[22rem]:inline">Copy</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="px-2 @[22rem]:px-4"
          onClick={() => {
            Analytics.trackButtonClick('open_recording_folder', 'meeting_details');
            onOpenMeetingFolder();
          }}
          title="Open Recording Folder"
        >
          <FolderOpen className="@[22rem]:mr-2" size={18} />
          <span className="hidden @[22rem]:inline">Recording</span>
        </Button>

        {meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="px-2 @[22rem]:px-4"
            onClick={() => {
              Analytics.trackButtonClick('identify_speakers', 'meeting_details');
              handleIdentifySpeakers();
            }}
            disabled={identifying || transcriptCount === 0}
            title="Tell the other people on the call apart (uses Deepgram)"
          >
            <Users className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">{identifying ? 'Identifying…' : 'Speakers'}</span>
          </Button>
        )}

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 px-2 @[22rem]:px-4"
            onClick={() => {
              Analytics.trackButtonClick('enhance_transcript', 'meeting_details');
              setShowRetranscribeDialog(true);
            }}
            title="Retranscribe to enhance your recorded audio"
          >
            <RefreshCw className="@[22rem]:mr-2" size={18} />
            <span className="hidden @[22rem]:inline">Enhance</span>
          </Button>
        )}
      </ButtonGroup>

      {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}
    </div>
  );
}
