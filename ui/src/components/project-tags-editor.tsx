import * as React from "react";
import { Plus } from "lucide-react";

import { TagBadge } from "@/components/tag-badge";
import { TagPickerDialog } from "@/components/tag-picker-dialog";
import { useSaveProjectTags, type ProjectTag } from "@/lib/project-mutations";

const VISIBLE = 12;

/** Shows a project's tags with remove buttons and a picker to add, remove or create tags. Pass `pickerOpen` to control the picker. */
export function ProjectTagsEditor({
  projectId,
  tags,
  pickerOpen,
  onPickerOpenChange,
}: {
  projectId: string;
  tags: ProjectTag[];
  pickerOpen?: boolean;
  onPickerOpenChange?: (open: boolean) => void;
}) {
  const [ownOpen, setOwnOpen] = React.useState(false);
  const open = pickerOpen ?? ownOpen;
  const setOpen = onPickerOpenChange ?? setOwnOpen;
  const [expanded, setExpanded] = React.useState(false);
  const saveM = useSaveProjectTags(projectId);
  const remove = (name: string) => saveM.mutate(tags.map((t) => t.name).filter((n) => n !== name));

  const shown = expanded ? tags : tags.slice(0, VISIBLE);
  const extra = tags.length - VISIBLE;

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        {tags.length === 0 ? <span className="text-sm text-muted-foreground">No tags</span> : null}
        {shown.map((t) => (
          <TagBadge key={t.id} name={t.name} color={t.color} onRemove={() => remove(t.name)} />
        ))}
        {extra > 0 ? (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="flex h-7 cursor-pointer items-center rounded-full border border-input px-2.5 text-xs text-muted-foreground transition-colors hover:border-dim hover:text-foreground"
          >
            {expanded ? "less" : `+${extra} more`}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="flex h-7 cursor-pointer items-center gap-1.5 rounded-full border border-dashed border-line-strong px-2.5 text-xs text-muted-foreground transition-colors hover:border-signal hover:text-foreground disabled:opacity-50"
        >
          <Plus className="size-3" strokeWidth={2.5} />
          Add tag
        </button>
      </div>
      {open ? <TagPickerDialog projectId={projectId} tags={tags} open={open} onOpenChange={setOpen} /> : null}
    </div>
  );
}
