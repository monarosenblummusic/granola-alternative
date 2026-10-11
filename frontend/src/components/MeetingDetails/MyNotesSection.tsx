"use client";

import { useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, NotebookPen, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useMeetingNotes } from '@/hooks/meeting-details/useMeetingNotes';

interface MyNotesSectionProps {
  meetingId: string;
  hasSummary: boolean;
  canEnhance: boolean;
  isEnhancing: boolean;
  /** Generate the summary; the backend includes the saved notes. */
  onEnhance: () => Promise<void>;
}

const SAVE_LABEL = { idle: '', saving: 'Saving…', saved: 'Saved', error: 'Not saved' } as const;

/**
 * Fork: the user's own notes above the AI notes, Granola style. Enhance writes
 * full notes from these notes plus the transcript.
 */
export function MyNotesSection({ meetingId, hasSummary, canEnhance, isEnhancing, onEnhance }: MyNotesSectionProps) {
  const { notes, setNotes, loaded, saveState, flush } = useMeetingNotes(meetingId);
  // null = follow the default: open until there are AI notes to read.
  const [open, setOpen] = useState<boolean | null>(null);
  const isOpen = open ?? !hasSummary;

  useEffect(() => setOpen(null), [meetingId]);

  const lineCount = notes.split('\n').filter((l) => l.trim()).length;

  const handleEnhance = async () => {
    await flush();
    await onEnhance();
  };

  return (
    <div className="border-b border-gray-200 bg-amber-50/40">
      <div className="flex items-center gap-2 px-4 py-2">
        <button
          type="button"
          onClick={() => setOpen(!isOpen)}
          className="flex flex-1 min-w-0 items-center gap-2 text-left text-sm font-medium text-gray-700 hover:text-gray-900"
          aria-expanded={isOpen}
        >
          {isOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          <NotebookPen size={16} className="text-gray-500" />
          <span>My notes</span>
          {!isOpen && loaded && (
            <span className="truncate text-xs font-normal text-gray-400">
              {lineCount ? `${lineCount} line${lineCount === 1 ? '' : 's'}` : 'empty'}
            </span>
          )}
          {isOpen && saveState !== 'idle' && (
            <span className={`text-xs font-normal ${saveState === 'error' ? 'text-red-600' : 'text-gray-400'}`}>
              {SAVE_LABEL[saveState]}
            </span>
          )}
        </button>
        <Button
          size="sm"
          className="gap-2 bg-blue-600 hover:bg-blue-700"
          onClick={() => void handleEnhance()}
          disabled={!canEnhance || isEnhancing}
          title={
            canEnhance
              ? 'Write full notes from your notes and the transcript'
              : 'Needs a transcript and a summary model'
          }
        >
          <Sparkles size={16} />
          {isEnhancing ? 'Enhancing…' : hasSummary ? 'Re-enhance' : 'Enhance'}
        </Button>
      </div>
      {isOpen && (
        <textarea
          aria-label="My notes"
          className="block w-full max-h-[40vh] min-h-[120px] resize-y bg-transparent px-6 pb-4 text-sm leading-relaxed text-gray-900 placeholder:text-gray-400 focus:outline-none"
          placeholder={'Your own notes for this meeting. Enhance keeps every point you write and fills in the details from the transcript.'}
          value={notes}
          disabled={!loaded}
          onChange={(e) => setNotes(e.target.value)}
        />
      )}
    </div>
  );
}
