import * as React from "react";
import { Plus } from "lucide-react";

import { apiErrorMessage } from "@/lib/tags";
import { TagBadge } from "@/components/tag-badge";
import { TagPickerDialog } from "@/components/tag-picker-dialog";
import { useSaveProjectTags, type ProjectTag } from "@/lib/project-mutations";

/** Shows a project's tags with remove buttons and a picker to add, remove or create tags. */
export function ProjectTagsEditor({ projectId, tags }: { projectId: string; tags: ProjectTag[] }) {
  const [open, setOpen] = React.useState(false);
  const saveM = useSaveProjectTags(projectId);
  const remove = (name: string) => saveM.mutate(tags.map((t) => t.name).filter((n) => n !== name));

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        {tags.length === 0 ? <span className="text-sm text-muted-foreground">No tags</span> : null}
        {tags.map((t) => (
          <TagBadge key={t.id} name={t.name} color={t.color} onRemove={() => remove(t.name)} />
        ))}
        <button
          type="button"
          onClick={() => setOpen(true)}
          disabled={saveM.isPending}
          className="flex h-7 cursor-pointer items-center gap-1.5 rounded-full border border-dashed border-line-strong px-2.5 text-xs text-muted-foreground transition-colors hover:border-signal hover:text-foreground disabled:opacity-50"
        >
          <Plus className="size-3" strokeWidth={2.5} />
          Add tag
        </button>
      </div>
      {saveM.isError ? (
        <div className="text-xs text-destructive">Could not update tags: {apiErrorMessage(saveM.error)}</div>
      ) : null}
      {open ? <TagPickerDialog projectId={projectId} tags={tags} open={open} onOpenChange={setOpen} /> : null}
    </div>
  );
}
