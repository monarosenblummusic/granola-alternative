"use client";

import { useEffect, useRef, useState } from 'react';
import { NotebookPen } from 'lucide-react';
import { NOTES_DRAFT_CLEARED_EVENT, readNotesDraft, writeNotesDraft } from '@/lib/meetingNotes';

interface NotepadPanelProps {
  /** Lock the notepad while the recording is being saved. */
  readOnly?: boolean;
}

/**
 * Fork: notepad next to the live transcript. Jot down the points that matter;
 * after the meeting, Enhance turns them into full notes using the transcript.
 */
export function NotepadPanel({ readOnly = false }: NotepadPanelProps) {
  const [text, setText] = useState('');
  const latest = useRef('');
  const loaded = useRef(false);

  useEffect(() => {
    const draft = readNotesDraft();
    latest.current = draft;
    setText(draft);
    loaded.current = true;

    const onCleared = () => {
      latest.current = '';
      setText('');
    };
    window.addEventListener(NOTES_DRAFT_CLEARED_EVENT, onCleared);
    return () => {
      window.removeEventListener(NOTES_DRAFT_CLEARED_EVENT, onCleared);
      writeNotesDraft(latest.current);
    };
  }, []);

  useEffect(() => {
    if (!loaded.current) return;
    const timer = setTimeout(() => writeNotesDraft(text), 300);
    return () => clearTimeout(timer);
  }, [text]);

  return (
    <div className="w-[42%] min-w-[280px] max-w-[640px] flex flex-col bg-white">
      <div className="flex items-center gap-2 px-6 pt-5 pb-2 text-sm font-medium text-gray-700">
        <NotebookPen size={16} className="text-gray-500" />
        Notes
      </div>
      <textarea
        aria-label="Meeting notes"
        className="flex-1 resize-none px-6 pt-2 pb-32 text-[15px] leading-relaxed text-gray-900 placeholder:text-gray-400 focus:outline-none disabled:bg-white disabled:text-gray-500"
        placeholder={'Write down what matters to you.\n\nAfter the meeting, Enhance turns these notes into full notes using the transcript.'}
        value={text}
        readOnly={readOnly}
        onChange={(e) => {
          latest.current = e.target.value;
          setText(e.target.value);
        }}
      />
    </div>
  );
}
