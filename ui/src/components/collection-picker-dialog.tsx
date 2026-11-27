import * as React from "react";
import { Check, Layers, Plus, Star } from "lucide-react";

import { useCollections } from "@/lib/collections";
import { useCollectionMembership, useFavourite, type Membership } from "@/lib/project-mutations";
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
 * Favourites is shown first and toggles the star.
 */
export function CollectionPickerDialog({
  projectId,
  collections,
  isFavourite,
  open,
  onOpenChange,
}: {
  projectId: string;
  collections: Membership[];
  isFavourite: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const allQ = useCollections();
  const { add, remove, create } = useCollectionMembership(projectId);
  const { toggle: toggleFavourite } = useFavourite(projectId, isFavourite);
  const [search, setSearch] = React.useState("");
  const [value, setValue] = React.useState("");

  // The cache is updated optimistically, so `collections` already reflects pending changes.
  const member = new Set(collections.map((c) => c.id));

  const favourites = (allQ.data ?? []).find((c) => c.is_favourites);
  const all = (allQ.data ?? []).filter((c) => !c.is_favourites);
  const typed = search.trim().replace(/\s+/g, " ");
  const exists = (allQ.data ?? []).some((c) => c.name.toLowerCase() === typed.toLowerCase());
  const createValue = `__create__ ${typed}`;
  const options = [favourites, ...all].filter((c): c is NonNullable<typeof c> => !!c);
  const matches = typed ? options.filter((c) => c.name.toLowerCase().includes(typed.toLowerCase())) : options;
  const matchCount = matches.length;
  const onlyMatch = matchCount === 1 ? matches[0].name : null;

  // With one match left, that match is the target, so Enter adds to it instead of creating.
  React.useEffect(() => {
    if (!open || !typed) return;
    if (onlyMatch) setValue(onlyMatch);
    else if (matchCount === 0 && !exists) setValue(createValue);
  }, [open, typed, onlyMatch, matchCount, exists, createValue]);

  return (
    <CommandDialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) {
          setSearch("");
          setValue("");
        }
      }}
      title="Collections"
      description="Pick collections to add or remove, or type a name to create one"
      value={value}
      onValueChange={setValue}
    >
      <CommandInput
        placeholder="Search or create a collection…"
        value={search}
        onValueChange={setSearch}
        maxLength={100}
        onKeyDown={(e) => {
          // Up on a filtered list moves to Create, so Enter with nothing highlighted creates
          // the typed collection instead of picking the one match.
          if (e.key === "ArrowUp" && typed && !exists && value !== createValue) {
            e.preventDefault();
            e.stopPropagation();
            setValue(createValue);
          }
        }}
      />
      <CommandList>
        <CommandEmpty>{typed ? "No matching collections." : "No collections yet. Type a name to create one."}</CommandEmpty>
        {typed && !exists ? (
          <CommandGroup heading="New">
            <CommandItem
              value={createValue}
              onSelect={() => {
                create.mutate(typed);
                setSearch("");
                setValue("");
              }}
              forceMount
            >
              <Plus />
              Create “{typed}” and add
            </CommandItem>
          </CommandGroup>
        ) : null}
        {favourites || all.length > 0 ? (
          <CommandGroup heading="Collections">
            {favourites ? (
              <CommandItem key={favourites.id} value={favourites.name} onSelect={toggleFavourite} data-checked={isFavourite}>
                <Star className="text-signal" />
                <span className="flex-1 truncate">{favourites.name}</span>
                {isFavourite ? <Check className="text-signal" /> : <span className="text-xs text-muted-foreground">{favourites.project_count}</span>}
              </CommandItem>
            ) : null}
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
