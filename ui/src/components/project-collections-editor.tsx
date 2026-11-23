import * as React from "react";
import { Link } from "react-router-dom";
import { Layers, Plus, X } from "lucide-react";

import { CollectionPickerDialog } from "@/components/collection-picker-dialog";
import { useCollectionMembership, type Membership } from "@/lib/project-mutations";

const VISIBLE = 12;

/** The project's collections as chips, plus a picker to add it to more (or a new one). */
export function ProjectCollectionsEditor({ projectId, collections }: { projectId: string; collections: Membership[] }) {
  const [open, setOpen] = React.useState(false);
  const [expanded, setExpanded] = React.useState(false);
  const { remove } = useCollectionMembership(projectId);

  const shown = expanded ? collections : collections.slice(0, VISIBLE);
  const extra = collections.length - VISIBLE;

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        {shown.map((c) => (
          <span
            key={c.id}
            className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-lg border border-input bg-card pr-1 pl-2.5 text-xs"
          >
            <Layers className="size-3 shrink-0 text-signal" />
            <Link to={`/collections/${c.id}`} className="truncate hover:text-signal">
              {c.name}
            </Link>
            <button
              type="button"
              onClick={() => remove.mutate(c.id)}
              aria-label={`Remove from collection ${c.name}`}
              className="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        {extra > 0 ? (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="flex h-7 cursor-pointer items-center rounded-lg border border-input px-2.5 text-xs text-muted-foreground transition-colors hover:border-dim hover:text-foreground"
          >
            {expanded ? "less" : `+${extra} more`}
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="flex h-7 cursor-pointer items-center gap-1.5 rounded-lg border border-dashed border-line-strong px-2.5 text-xs text-muted-foreground transition-colors hover:border-signal hover:text-foreground"
        >
          <Plus className="size-3" strokeWidth={2.5} />
          Add to collection
        </button>
      </div>
      {open ? (
        <CollectionPickerDialog projectId={projectId} collections={collections} open={open} onOpenChange={setOpen} />
      ) : null}
    </div>
  );
}
