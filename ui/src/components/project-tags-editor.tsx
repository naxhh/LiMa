import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus, Tag as TagIcon } from "lucide-react";

import { apiJson } from "@/lib/api";
import { TAGS_QUERY_KEY, apiErrorMessage, useAllTags } from "@/lib/tags";
import { Button } from "@/components/ui/button";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { TagBadge } from "@/components/tag-badge";

type ProjectTag = { id: string; name: string; color: string };

/** Shows a project's tags with remove buttons and a picker to add or create tags. */
export function ProjectTagsEditor({ projectId, tags }: { projectId: string; tags: ProjectTag[] }) {
  const qc = useQueryClient();
  const allTagsQ = useAllTags();
  const [open, setOpen] = React.useState(false);
  const [search, setSearch] = React.useState("");

  const saveM = useMutation({
    mutationFn: (names: string[]) => apiJson<void>("PATCH", `/projects/${projectId}`, { tags: names }),
    onSuccess: async () => {
      await Promise.all([
        qc.invalidateQueries({ queryKey: ["project", projectId] }),
        qc.invalidateQueries({ queryKey: ["projects"] }),
        qc.invalidateQueries({ queryKey: TAGS_QUERY_KEY }),
      ]);
    },
  });

  const current = tags.map((t) => t.name);
  const has = (name: string) => current.some((n) => n.toLowerCase() === name.toLowerCase());

  const add = (name: string) => {
    const clean = name.trim().replace(/\s+/g, " ");
    if (!clean || has(clean)) return;
    saveM.mutate([...current, clean]);
    setSearch("");
  };
  const remove = (name: string) => saveM.mutate(current.filter((n) => n !== name));

  const available = (allTagsQ.data ?? []).filter((t) => !has(t.name));
  const typed = search.trim().replace(/\s+/g, " ");
  const exists = (allTagsQ.data ?? []).some((t) => t.name.toLowerCase() === typed.toLowerCase());

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-2">
        {tags.length === 0 ? <span className="text-sm text-muted-foreground">No tags</span> : null}
        {tags.map((t) => (
          <TagBadge key={t.id} name={t.name} color={t.color} onRemove={() => remove(t.name)} />
        ))}
        <Button variant="outline" size="xs" onClick={() => setOpen(true)} disabled={saveM.isPending}>
          <Plus />
          Add tag
        </Button>
      </div>
      {saveM.isError ? (
        <div className="text-xs text-destructive">Could not update tags: {apiErrorMessage(saveM.error)}</div>
      ) : null}

      <CommandDialog
        open={open}
        onOpenChange={(o) => {
          setOpen(o);
          if (!o) setSearch("");
        }}
        title="Add tag"
        description="Search existing tags or create a new one"
      >
        <CommandInput placeholder="Search or create a tag…" value={search} onValueChange={setSearch} maxLength={64} />
        <CommandList>
          <CommandEmpty>{typed ? "No matching tags." : "No more tags. Type a name to create one."}</CommandEmpty>
          {typed && !exists ? (
            <CommandGroup heading="New">
              <CommandItem value={`__create__ ${typed}`} onSelect={() => add(typed)} forceMount>
                <Plus />
                Create “{typed}”
              </CommandItem>
            </CommandGroup>
          ) : null}
          {available.length > 0 ? (
            <CommandGroup heading="Tags">
              {available.map((t) => (
                <CommandItem key={t.id} value={t.name} onSelect={() => add(t.name)}>
                  <TagIcon style={{ color: t.color }} />
                  <span className="flex-1 truncate">{t.name}</span>
                  <span className="text-xs text-muted-foreground">{t.project_count}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          ) : null}
        </CommandList>
      </CommandDialog>
    </div>
  );
}
