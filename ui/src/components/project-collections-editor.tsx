import * as React from "react";
import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Layers, Plus, X } from "lucide-react";

import { apiJson, apiNoBody } from "@/lib/api";
import { invalidateCollections, useCollections, type Collection } from "@/lib/collections";
import { apiErrorMessage } from "@/lib/tags";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

type Membership = { id: string; name: string };

/** The project's collections as chips, plus a picker to add it to more (or a new one). */
export function ProjectCollectionsEditor({ projectId, collections }: { projectId: string; collections: Membership[] }) {
  const qc = useQueryClient();
  const allQ = useCollections();
  const [open, setOpen] = React.useState(false);
  const [search, setSearch] = React.useState("");

  const addM = useMutation({
    mutationFn: (collectionId: string) =>
      apiNoBody("POST", `/collections/${collectionId}/projects`, { project_id: projectId }),
    onSuccess: () => invalidateCollections(qc, projectId),
  });
  const removeM = useMutation({
    mutationFn: (collectionId: string) => apiNoBody("DELETE", `/collections/${collectionId}/projects/${projectId}`),
    onSuccess: () => invalidateCollections(qc, projectId),
  });
  const createM = useMutation({
    mutationFn: async (name: string) => {
      const c = await apiJson<Collection>("POST", "/collections", { name });
      await apiNoBody("POST", `/collections/${c.id}/projects`, { project_id: projectId });
    },
    onSuccess: () => invalidateCollections(qc, projectId),
  });

  const memberIds = new Set(collections.map((c) => c.id));
  const available = (allQ.data ?? []).filter((c) => !memberIds.has(c.id));
  const typed = search.trim().replace(/\s+/g, " ");
  const exists = (allQ.data ?? []).some((c) => c.name.toLowerCase() === typed.toLowerCase());
  const busy = addM.isPending || removeM.isPending || createM.isPending;
  const error = addM.error ?? removeM.error ?? createM.error;

  const close = () => {
    setOpen(false);
    setSearch("");
  };

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        {collections.map((c) => (
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
              onClick={() => removeM.mutate(c.id)}
              disabled={busy}
              aria-label={`Remove from collection ${c.name}`}
              className="grid size-5 cursor-pointer place-items-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        <button
          type="button"
          onClick={() => setOpen(true)}
          disabled={busy}
          className="flex h-7 cursor-pointer items-center gap-1.5 rounded-lg border border-dashed border-line-strong px-2.5 text-xs text-muted-foreground transition-colors hover:border-signal hover:text-foreground disabled:opacity-50"
        >
          <Plus className="size-3" strokeWidth={2.5} />
          Add to collection
        </button>
      </div>
      {error ? <div className="text-xs text-destructive">Could not update collections: {apiErrorMessage(error)}</div> : null}

      <CommandDialog
        open={open}
        onOpenChange={(o) => (o ? setOpen(true) : close())}
        title="Add to collection"
        description="Pick a collection or create a new one"
      >
        <CommandInput placeholder="Search or create a collection…" value={search} onValueChange={setSearch} maxLength={100} />
        <CommandList>
          <CommandEmpty>{typed ? "No matching collections." : "No other collections. Type a name to create one."}</CommandEmpty>
          {typed && !exists ? (
            <CommandGroup heading="New">
              <CommandItem
                value={`__create__ ${typed}`}
                onSelect={() => {
                  createM.mutate(typed);
                  close();
                }}
                forceMount
              >
                <Plus />
                Create “{typed}” and add
              </CommandItem>
            </CommandGroup>
          ) : null}
          {available.length > 0 ? (
            <CommandGroup heading="Collections">
              {available.map((c) => (
                <CommandItem
                  key={c.id}
                  value={c.name}
                  onSelect={() => {
                    addM.mutate(c.id);
                    close();
                  }}
                >
                  <Layers />
                  <span className="flex-1 truncate">{c.name}</span>
                  <span className="text-xs text-muted-foreground">{c.project_count}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          ) : null}
        </CommandList>
      </CommandDialog>
    </div>
  );
}
