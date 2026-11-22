import * as React from "react";
import { Check, Plus } from "lucide-react";

import { useAllTags } from "@/lib/tags";
import { useSaveProjectTags, type ProjectTag } from "@/lib/project-mutations";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

/**
 * Searchable list of every tag; picking one toggles it on the project. Typing a new
 * name offers to create it. Stays open so several tags can be changed in a row.
 */
export function TagPickerDialog({
  projectId,
  tags,
  open,
  onOpenChange,
}: {
  projectId: string;
  tags: ProjectTag[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const allTagsQ = useAllTags();
  const saveM = useSaveProjectTags(projectId);
  const [search, setSearch] = React.useState("");

  // The cache is updated optimistically, so `tags` already reflects pending changes.
  const current = tags.map((t) => t.name);
  const has = (name: string) => current.some((n) => n.toLowerCase() === name.toLowerCase());

  const toggle = (name: string) => {
    const next = has(name) ? current.filter((n) => n.toLowerCase() !== name.toLowerCase()) : [...current, name];
    saveM.mutate(next);
  };

  const typed = search.trim().replace(/\s+/g, " ");
  const all = allTagsQ.data ?? [];
  const exists = all.some((t) => t.name.toLowerCase() === typed.toLowerCase());

  return (
    <CommandDialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) setSearch("");
      }}
      title="Tags"
      description="Pick tags to add or remove, or type a name to create one"
    >
      <CommandInput placeholder="Search or create a tag…" value={search} onValueChange={setSearch} maxLength={64} />
      <CommandList>
        <CommandEmpty>{typed ? "No matching tags." : "No tags yet. Type a name to create one."}</CommandEmpty>
        {typed && !exists ? (
          <CommandGroup heading="New">
            <CommandItem
              value={`__create__ ${typed}`}
              onSelect={() => {
                toggle(typed);
                setSearch("");
              }}
              forceMount
            >
              <Plus />
              Create “{typed}”
            </CommandItem>
          </CommandGroup>
        ) : null}
        {all.length > 0 ? (
          <CommandGroup heading="Tags">
            {all.map((t) => {
              const on = has(t.name);
              return (
                <CommandItem key={t.id} value={t.name} onSelect={() => toggle(t.name)} data-checked={on}>
                  <span className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: t.color }} />
                  <span className="flex-1 truncate">{t.name}</span>
                  {on ? <Check className="text-signal" /> : <span className="text-xs text-muted-foreground">{t.project_count}</span>}
                </CommandItem>
              );
            })}
          </CommandGroup>
        ) : null}
      </CommandList>
    </CommandDialog>
  );
}
