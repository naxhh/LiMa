import * as React from "react";
import { Plus, X } from "lucide-react";

import { matchTags, useAllTags, type Tag } from "@/lib/tags";
import { cn } from "@/lib/utils";

type Props = {
  value: string[];
  onChange: (tags: string[]) => void;
};

type Option = { kind: "tag"; tag: Tag } | { kind: "new"; name: string };

const MAX_SUGGESTIONS = 8;

/** Comma, pipe or newline separated free text, the way the old textarea split it. */
function splitTags(text: string): string[] {
  return text
    .split(/[,|\n]/g)
    .map((s) => s.trim().replace(/\s+/g, " "))
    .filter(Boolean);
}

/**
 * Tag chips with suggestions from the existing tags. Typing filters case-insensitively and
 * ranks prefix matches first; a name that doesn't exist yet can still be created. Enter or
 * Tab accepts the highlighted suggestion, comma or Enter turns free text into a chip.
 */
export function TagChipInput({ value, onChange }: Props) {
  const allQ = useAllTags();
  const all = allQ.data ?? [];
  const [input, setInput] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const [highlight, setHighlight] = React.useState(-1);
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  const listId = React.useId();

  const selected = new Set(value.map((v) => v.toLowerCase()));
  const has = (name: string) => selected.has(name.toLowerCase());

  /** Existing names win, so `benchy` picks `Benchy` instead of making a second tag. */
  const canonical = (name: string) => {
    const clean = name.trim().replace(/\s+/g, " ");
    return all.find((t) => t.name.toLowerCase() === clean.toLowerCase())?.name ?? clean;
  };

  const add = (names: string[]) => {
    const next = [...value];
    for (const raw of names) {
      const name = canonical(raw);
      if (!name || name.length > 64 || next.some((v) => v.toLowerCase() === name.toLowerCase())) continue;
      next.push(name);
    }
    if (next.length !== value.length) onChange(next);
  };

  const remove = (name: string) => onChange(value.filter((v) => v.toLowerCase() !== name.toLowerCase()));

  const clearInput = () => {
    setInput("");
    setHighlight(-1);
    setOpen(false);
  };

  const commit = (text = input) => {
    add(splitTags(text));
    clearInput();
  };

  const query = input.trim();
  const exists = query.length > 0 && all.some((t) => t.name.toLowerCase() === query.toLowerCase());
  // Already-added tags are left out: the list only offers what a pick would add.
  const options: Option[] = [
    ...matchTags(all, query)
      .filter((t) => !has(t.name))
      .slice(0, MAX_SUGGESTIONS)
      .map((tag) => ({ kind: "tag" as const, tag })),
    ...(query.length > 0 && !exists ? [{ kind: "new" as const, name: query }] : []),
  ];

  const accept = (option?: Option) => {
    if (!option) return commit();
    if (option.kind === "new") return commit(option.name);
    add([option.tag.name]);
    clearInput();
  };

  const move = (delta: 1 | -1) => {
    if (options.length === 0) return setHighlight(-1);
    setOpen(true);
    setHighlight((h) => {
      if (h < 0) return delta > 0 ? 0 : options.length - 1;
      return (h + delta + options.length) % options.length;
    });
  };

  return (
    <div
      className="relative"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <div
        className="flex min-h-9 w-full flex-wrap items-center gap-1.5 rounded-md border border-input bg-transparent px-2 py-1.5 text-sm focus-within:border-ring"
        onClick={() => inputRef.current?.focus()}
      >
        {value.map((name) => {
          const tag = all.find((t) => t.name.toLowerCase() === name.toLowerCase());
          return (
            <span
              key={name.toLowerCase()}
              className="inline-flex max-w-full items-center gap-1 rounded-md border bg-secondary px-1.5 py-0.5 text-xs"
            >
              {tag ? <span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: tag.color }} /> : null}
              <span className="truncate">{name}</span>
              <button
                type="button"
                aria-label={`Remove ${name}`}
                className="rounded-sm text-muted-foreground hover:text-foreground"
                onMouseDown={(e) => e.preventDefault()}
                onClick={(e) => {
                  e.stopPropagation();
                  remove(name);
                  inputRef.current?.focus();
                }}
              >
                <X className="size-3" />
              </button>
            </span>
          );
        })}
        <input
          ref={inputRef}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open}
          aria-controls={listId}
          aria-activedescendant={highlight >= 0 ? `${listId}-${highlight}` : undefined}
          value={input}
          maxLength={64}
          placeholder={value.length === 0 ? "Type to add tags…" : ""}
          className="min-w-[10ch] flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          onChange={(e) => {
            setInput(e.target.value);
            setOpen(true);
            setHighlight(-1);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              move(1);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              move(-1);
            } else if (e.key === "Enter") {
              e.preventDefault();
              if (highlight >= 0) accept(options[highlight]);
              else commit();
            } else if (e.key === "Tab") {
              if (e.shiftKey || options.length === 0) return;
              e.preventDefault();
              accept(highlight >= 0 ? options[highlight] : options[0]);
            } else if (e.key === ",") {
              e.preventDefault();
              commit();
            } else if (e.key === "Backspace" && input === "" && value.length > 0) {
              e.preventDefault();
              remove(value[value.length - 1]);
            } else if (e.key === "Escape" && open) {
              e.preventDefault();
              setOpen(false);
            }
          }}
          onPaste={(e) => {
            const text = e.clipboardData.getData("text");
            if (!/[,|\n]/.test(text)) return;
            e.preventDefault();
            commit([input, text].filter(Boolean).join(", "));
          }}
        />
      </div>

      {open ? (
        <div className="absolute z-50 mt-1 w-full overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md">
          {options.length === 0 ? (
            <div className="px-2 py-3 text-sm text-muted-foreground">
              {allQ.isError
                ? "Couldn't load tags. Type a name to create one."
                : allQ.isLoading
                  ? "Loading tags…"
                  : query
                    ? "No matching tags."
                    : "No tags yet. Type a name to create one."}
            </div>
          ) : (
            <ul id={listId} role="listbox" aria-label="Tag suggestions" className="max-h-56 overflow-y-auto p-1">
              {options.map((option, i) => (
                <li
                  key={option.kind === "new" ? `new:${option.name}` : option.tag.id}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === highlight}
                  className={cn(
                    "flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm select-none",
                    i === highlight && "bg-accent text-accent-foreground",
                  )}
                  onMouseDown={(e) => e.preventDefault()}
                  onMouseEnter={() => setHighlight(i)}
                  onClick={() => accept(option)}
                >
                  {option.kind === "new" ? (
                    <>
                      <Plus className="size-4 shrink-0" />
                      <span className="flex-1 truncate">Create “{option.name}”</span>
                      <span className="font-mono text-[10px] tracking-[0.1em] text-dim uppercase">new</span>
                    </>
                  ) : (
                    <>
                      <span className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: option.tag.color }} />
                      <span className="flex-1 truncate">{option.tag.name}</span>
                      <span className="text-xs text-muted-foreground">{option.tag.project_count}</span>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
    </div>
  );
}
