"use client";

import { useEffect, useState } from 'react';
import { Loader2, UserPen } from 'lucide-react';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { defaultSpeakerLabel } from '@/lib/speaker';
import { changedNames, getMeetingSpeakers, MeetingSpeaker, renameSpeakers } from '@/lib/speakerNames';

interface RenameSpeakersDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  meetingId: string;
}

/**
 * Fork: give the speakers of one meeting real names. Names only change how the
 * transcript is shown, copied and summarised; the stored transcript is untouched.
 */
export function RenameSpeakersDialog({ open, onOpenChange, meetingId }: RenameSpeakersDialogProps) {
  const [speakers, setSpeakers] = useState<MeetingSpeaker[] | null>(null);
  const [edited, setEdited] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setSpeakers(null);
    getMeetingSpeakers(meetingId)
      .then((list) => {
        if (cancelled) return;
        setSpeakers(list);
        setEdited(Object.fromEntries(list.map((s) => [s.speaker, s.display_name ?? ''])));
      })
      .catch((err) => {
        if (cancelled) return;
        toast.error(`Could not load speakers: ${err}`);
        onOpenChange(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, meetingId, onOpenChange]);

  const handleSave = async () => {
    if (!speakers) return;
    const changes = changedNames(speakers, edited);
    if (changes.length === 0) {
      onOpenChange(false);
      return;
    }
    setSaving(true);
    try {
      await renameSpeakers(meetingId, changes);
      toast.success(changes.length === 1 ? 'Speaker renamed' : `${changes.length} speakers renamed`);
      onOpenChange(false);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !saving && onOpenChange(o)}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <UserPen className="h-5 w-5 text-blue-600" />
            Name speakers
          </DialogTitle>
          <DialogDescription>
            Names are used in this meeting&apos;s transcript, copies and summaries. Leave a box empty to keep the default label.
          </DialogDescription>
        </DialogHeader>

        <form
          className="max-h-[50vh] overflow-y-auto space-y-4 py-2 pr-1"
          onSubmit={(e) => {
            e.preventDefault();
            void handleSave();
          }}
        >
          {speakers === null && (
            <div className="flex items-center gap-2 text-sm text-gray-500">
              <Loader2 className="h-4 w-4 animate-spin" /> Loading speakers…
            </div>
          )}
          {speakers?.length === 0 && (
            <p className="text-sm text-gray-500">This transcript has no labelled speakers.</p>
          )}
          {speakers?.map((s) => (
            <div key={s.speaker} className="space-y-1">
              <label htmlFor={`speaker-${s.speaker}`} className="flex items-baseline justify-between text-sm font-medium">
                <span>{defaultSpeakerLabel(s.speaker)}</span>
                <span className="text-xs font-normal text-gray-400">
                  {s.line_count} line{s.line_count === 1 ? '' : 's'}
                </span>
              </label>
              <Input
                id={`speaker-${s.speaker}`}
                value={edited[s.speaker] ?? ''}
                placeholder={defaultSpeakerLabel(s.speaker)}
                maxLength={60}
                disabled={saving}
                onChange={(e) => setEdited((prev) => ({ ...prev, [s.speaker]: e.target.value }))}
              />
              {s.sample && (
                <p className="text-xs text-gray-500 line-clamp-2" title={s.sample}>
                  &ldquo;{s.sample}&rdquo;
                </p>
              )}
            </div>
          ))}
          <button type="submit" hidden />
        </form>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()} disabled={saving || !speakers?.length} className="bg-blue-600 hover:bg-blue-700">
            {saving && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
            Save names
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
