import * as React from "react";
import { Check, Layers, Plus } from "lucide-react";

import { useCollections } from "@/lib/collections";
import { useCollectionMembership, type Membership } from "@/lib/project-mutations";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

/**
 * Searchable list of the user's collections; picking one toggles the project in it.
 * Typing a new name offers to create the collection with the project in it.
 * Favourites is left out: the star handles it.
 */
export function CollectionPickerDialog({
  projectId,
  collections,
  open,
  onOpenChange,
}: {
  projectId: string;
  collections: Membership[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const allQ = useCollections();
  const { add, remove, create } = useCollectionMembership(projectId);
  const [search, setSearch] = React.useState("");

  // The cache is updated optimistically, so `collections` already reflects pending changes.
  const member = new Set(collections.map((c) => c.id));

  const all = (allQ.data ?? []).filter((c) => !c.is_favourites);
  const typed = search.trim().replace(/\s+/g, " ");
  const exists = (allQ.data ?? []).some((c) => c.name.toLowerCase() === typed.toLowerCase());

  return (
    <CommandDialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) setSearch("");
      }}
      title="Collections"
      description="Pick collections to add or remove, or type a name to create one"
    >
      <CommandInput placeholder="Search or create a collection…" value={search} onValueChange={setSearch} maxLength={100} />
      <CommandList>
        <CommandEmpty>{typed ? "No matching collections." : "No collections yet. Type a name to create one."}</CommandEmpty>
        {typed && !exists ? (
          <CommandGroup heading="New">
            <CommandItem
              value={`__create__ ${typed}`}
              onSelect={() => {
                create.mutate(typed);
                setSearch("");
              }}
              forceMount
            >
              <Plus />
              Create “{typed}” and add
            </CommandItem>
          </CommandGroup>
        ) : null}
        {all.length > 0 ? (
          <CommandGroup heading="Collections">
            {all.map((c) => {
              const on = member.has(c.id);
              return (
                <CommandItem
                  key={c.id}
                  value={c.name}
                  onSelect={() => (on ? remove.mutate(c.id) : add.mutate(c.id))}
                  data-checked={on}
                >
                  <Layers />
                  <span className="flex-1 truncate">{c.name}</span>
                  {on ? <Check className="text-signal" /> : <span className="text-xs text-muted-foreground">{c.project_count}</span>}
                </CommandItem>
              );
            })}
          </CommandGroup>
        ) : null}
      </CommandList>
    </CommandDialog>
  );
}
