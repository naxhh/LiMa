import * as React from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Command as CommandPrimitive } from "cmdk";
import { Box, Filter, Hash, Layers, Search, Star } from "lucide-react";

import type { components } from "@/gen/openapi";
import { apiGet } from "@/lib/api";
import { useCollections } from "@/lib/collections";
import { thumbUrl } from "@/lib/media";
import { FILTERS, currentToken, quote, replaceToken, searchType, stripType } from "@/lib/search-syntax";
import { useAllTags } from "@/lib/tags";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";

type ListProjectsResponse = components["schemas"]["ListProjectsResponse"];

const FORMATS = ["STL", "3MF", "STEP", "OBJ", "GCODE"];
const SOURCES = [
  { value: "printables", label: "Printables" },
  { value: "makerworld", label: "MakerWorld" },
  { value: "thingiverse", label: "Thingiverse" },
];

function useDebounced<T>(value: T, ms: number) {
  const [v, setV] = React.useState(value);
  React.useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

const item =
  "flex cursor-default items-center gap-2.5 rounded-md px-2.5 py-2 text-sm outline-none select-none data-[selected=true]:bg-secondary [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground";
const group = "px-1.5 py-1 [&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:font-mono [&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:tracking-[0.1em] [&_[cmdk-group-heading]]:text-dim [&_[cmdk-group-heading]]:uppercase";

/**
 * ⌘K search: type words or filters (`#tag`, `in:collection`, `is:fav`, `format:stl`, ...).
 * Suggests values for the filter being typed, matching projects, tags and collections.
 * Enter searches the library.
 */
export function SearchPalette({ open, onOpenChange, initial }: { open: boolean; onOpenChange: (open: boolean) => void; initial: string }) {
  const navigate = useNavigate();
  const [value, setValue] = React.useState(initial);
  const inputRef = React.useRef<HTMLInputElement>(null);
  React.useEffect(() => {
    if (open) setValue(initial ? `${initial.trimEnd()} ` : "");
  }, [open, initial]);

  const tags = useAllTags().data ?? [];
  const collections = useCollections().data ?? [];
  const token = currentToken(value);
  const mode = searchType(value);
  const searchQuery = stripType(value).trim();
  const searchText = searchQuery.toLowerCase();
  const debounced = useDebounced(value.trim(), 200);
  const projectsQ = useQuery({
    queryKey: ["palette-projects", debounced],
    queryFn: () => apiGet<ListProjectsResponse>(`/projects?limit=6&query=${encodeURIComponent(debounced)}`),
    enabled: open && debounced.length > 0,
    staleTime: 10_000,
  });

  const close = () => onOpenChange(false);
  const search = (q: string) => {
    close();
    const trimmed = q.trim();
    navigate(trimmed ? `/?query=${encodeURIComponent(trimmed)}` : "/");
  };
  const complete = (replacement: string) => {
    setValue(replaceToken(value, token, replacement));
    inputRef.current?.focus();
  };

  // Appends a filter prefix to start typing its value.
  const insert = (prefix: string) => {
    setValue(`${value.trimEnd()}${value.trim() ? " " : ""}${prefix}`);
    inputRef.current?.focus();
  };

  const match = (name: string) => name.toLowerCase().includes(token.value);
  const tagHits = tags.filter((t) => match(t.name)).slice(0, 8);
  const collectionHits = collections.filter((c) => match(c.name)).slice(0, 8);
  const projects = projectsQ.data?.items ?? [];

  // Suggestions for the filter being typed (or the chosen `type:` kind).
  let suggestions: React.ReactNode = null;
  if (mode === "tag") {
    const hits = tags.filter((t) => !searchText || t.name.toLowerCase().includes(searchText)).slice(0, 30);
    suggestions = (
      <CommandPrimitive.Group heading="Tags" className={group}>
        {hits.map((t) => (
          <CommandPrimitive.Item
            key={t.id}
            value={`tag-${t.id}`}
            className={item}
            onSelect={() => {
              close();
              navigate(`/?query=${encodeURIComponent(`#${quote(t.name)}`)}`);
            }}
          >
            <span className="size-2.5 shrink-0 rounded-full" style={{ background: t.color }} />
            {t.name}
            <span className="ml-auto font-mono text-[11px] text-dim">{t.project_count}</span>
          </CommandPrimitive.Item>
        ))}
      </CommandPrimitive.Group>
    );
  } else if (mode === "collection") {
    const hits = collections.filter((c) => !searchText || c.name.toLowerCase().includes(searchText)).slice(0, 30);
    suggestions = (
      <CommandPrimitive.Group heading="Collections" className={group}>
        {hits.map((c) => (
          <CommandPrimitive.Item
            key={c.id}
            value={`collection-${c.id}`}
            className={item}
            onSelect={() => {
              close();
              navigate(`/collections/${c.id}`);
            }}
          >
            {c.is_favourites ? <Star /> : <Layers />}
            {c.name}
            <span className="ml-auto font-mono text-[11px] text-dim">{c.project_count}</span>
          </CommandPrimitive.Item>
        ))}
      </CommandPrimitive.Group>
    );
  } else if (token.key === "type") {
    const options = [
      { value: "type:project", label: "Projects", Icon: Box },
      { value: "type:tag", label: "Tags", Icon: Hash },
      { value: "type:collection", label: "Collections", Icon: Layers },
    ];
    suggestions = (
      <CommandPrimitive.Group heading="Search for" className={group}>
        {options
          .filter((o) => o.value.toLowerCase().includes(token.value) || o.label.toLowerCase().includes(token.value))
          .map((o) => (
            <CommandPrimitive.Item key={o.value} value={o.value} className={item} onSelect={() => complete(o.value)}>
              <o.Icon />
              {o.label}
              <span className="ml-auto font-mono text-[11px] text-dim">{o.value}</span>
            </CommandPrimitive.Item>
          ))}
      </CommandPrimitive.Group>
    );
  } else if (token.key === "tag") {
    suggestions = (
      <CommandPrimitive.Group heading="Tags" className={group}>
        {tagHits.map((t) => (
          <CommandPrimitive.Item key={t.id} value={`tag-${t.id}`} className={item} onSelect={() => complete(`#${quote(t.name)}`)}>
            <span className="size-2.5 shrink-0 rounded-full" style={{ background: t.color }} />
            {t.name}
            <span className="ml-auto font-mono text-[11px] text-dim">{t.project_count}</span>
          </CommandPrimitive.Item>
        ))}
      </CommandPrimitive.Group>
    );
  } else if (token.key === "in") {
    suggestions = (
      <CommandPrimitive.Group heading="Collections" className={group}>
        {collectionHits.map((c) => (
          <CommandPrimitive.Item
            key={c.id}
            value={`in-${c.id}`}
            className={item}
            onSelect={() => complete(`in:${c.is_favourites ? "favourites" : quote(c.name)}`)}
          >
            {c.is_favourites ? <Star /> : <Layers />}
            {c.name}
            <span className="ml-auto font-mono text-[11px] text-dim">{c.project_count}</span>
          </CommandPrimitive.Item>
        ))}
      </CommandPrimitive.Group>
    );
  } else if (token.key === "format" || token.key === "from" || token.key === "is" || token.key === "has") {
    const options =
      token.key === "format"
        ? FORMATS.map((f) => ({ value: `format:${f.toLowerCase()}`, label: f }))
        : token.key === "from"
          ? SOURCES.map((s) => ({ value: `from:${s.value}`, label: s.label }))
          : token.key === "is"
            ? [{ value: "is:fav", label: "Favourites" }]
            : [
                { value: "has:image", label: "Has pictures" },
                { value: "no:image", label: "No pictures" },
              ];
    suggestions = (
      <CommandPrimitive.Group heading="Filter" className={group}>
        {options
          .filter((o) => o.value.toLowerCase().includes(token.value) || o.label.toLowerCase().includes(token.value))
          .map((o) => (
            <CommandPrimitive.Item key={o.value} value={o.value} className={item} onSelect={() => complete(o.value)}>
              <Filter />
              {o.label}
              <span className="ml-auto font-mono text-[11px] text-dim">{o.value}</span>
            </CommandPrimitive.Item>
          ))}
      </CommandPrimitive.Group>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false} className="top-[15%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl">
        <DialogHeader className="sr-only">
          <DialogTitle>Search</DialogTitle>
          <DialogDescription>Search projects, or filter with #tag, in:collection, is:fav and more.</DialogDescription>
        </DialogHeader>
        <CommandPrimitive shouldFilter={false} loop className="flex flex-col">
          <div className="flex items-center gap-2.5 border-b px-4">
            <Search className="size-4 shrink-0 text-dim" />
            <CommandPrimitive.Input
              ref={inputRef}
              value={value}
              onValueChange={setValue}
              placeholder="Search, or filter: #tag in:collection is:fav format:stl…"
              className="h-13 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-dim"
            />
          </div>
          <CommandPrimitive.List className="max-h-[min(60vh,440px)] overflow-y-auto pb-1.5">
            {suggestions}
            {mode === "project" && token.key !== null ? (
              <CommandPrimitive.Group className={group}>
                <CommandPrimitive.Item value="search" className={item} onSelect={() => search(searchQuery)}>
                  <Search />
                  {searchQuery ? (
                    <>
                      Search the library for <span className="truncate font-medium">{searchQuery}</span>
                    </>
                  ) : (
                    "Show the whole library"
                  )}
                </CommandPrimitive.Item>
              </CommandPrimitive.Group>
            ) : null}

            {mode === "project" && token.key === null ? (
              <>
                <CommandPrimitive.Group className={group}>
                  <CommandPrimitive.Item value="search" className={item} onSelect={() => search(searchQuery)}>
                    <Search />
                    {searchQuery ? (
                      <>
                        Search the library for <span className="truncate font-medium">{searchQuery}</span>
                      </>
                    ) : (
                      "Show the whole library"
                    )}
                    <kbd className="ml-auto font-mono text-[10px] text-dim">↵</kbd>
                  </CommandPrimitive.Item>
                </CommandPrimitive.Group>

                {projects.length > 0 ? (
                  <CommandPrimitive.Group heading="Projects" className={group}>
                    {projects.map((p) => {
                      const cover = p.main_image_id ?? p.preview_id;
                      return (
                        <CommandPrimitive.Item
                          key={p.id}
                          value={`project-${p.id}`}
                          className={item}
                          onSelect={() => {
                            close();
                            navigate(`/projects/${p.id}`);
                          }}
                        >
                          <span className="grid size-7 shrink-0 place-items-center overflow-hidden rounded border bg-thumb">
                            {cover ? <img src={thumbUrl(p.id, cover)} alt="" className="size-full object-cover" /> : <Box className="size-3.5" />}
                          </span>
                          <span className="min-w-0 flex-1 truncate">{p.name}</span>
                          <span className="font-mono text-[11px] text-dim">{p.model_formats.join(" ")}</span>
                        </CommandPrimitive.Item>
                      );
                    })}
                  </CommandPrimitive.Group>
                ) : null}

                {token.value && tagHits.length > 0 ? (
                  <CommandPrimitive.Group heading="Filter by tag" className={group}>
                    {tagHits.slice(0, 4).map((t) => (
                      <CommandPrimitive.Item key={t.id} value={`tagword-${t.id}`} className={item} onSelect={() => complete(`#${quote(t.name)}`)}>
                        <Hash />
                        {t.name}
                        <span className="ml-auto font-mono text-[11px] text-dim">#{quote(t.name)}</span>
                      </CommandPrimitive.Item>
                    ))}
                  </CommandPrimitive.Group>
                ) : null}

                {token.value && collectionHits.length > 0 ? (
                  <CommandPrimitive.Group heading="Collections" className={group}>
                    {collectionHits.slice(0, 4).map((c) => (
                      <CommandPrimitive.Item
                        key={c.id}
                        value={`collection-${c.id}`}
                        className={item}
                        onSelect={() => {
                          close();
                          navigate(`/collections/${c.id}`);
                        }}
                      >
                        {c.is_favourites ? <Star /> : <Layers />}
                        {c.name}
                        <span className="ml-auto font-mono text-[11px] text-dim">open</span>
                      </CommandPrimitive.Item>
                    ))}
                  </CommandPrimitive.Group>
                ) : null}

                {!token.value ? (
                  <CommandPrimitive.Group heading="Filters" className={group}>
                    {FILTERS.map((f) => (
                      <CommandPrimitive.Item key={f.insert} value={`help-${f.insert}`} className={item} onSelect={() => insert(f.insert)}>
                        <Filter />
                        {f.label}
                        <span className="ml-auto font-mono text-[11px] text-dim">{f.example}</span>
                      </CommandPrimitive.Item>
                    ))}
                  </CommandPrimitive.Group>
                ) : null}
              </>
            ) : null}
          </CommandPrimitive.List>
          <div className="flex flex-wrap gap-x-3 gap-y-1 border-t px-4 py-2 font-mono text-[10px] text-dim">
            <span>↵ search</span>
            <span>#tag</span>
            <span>type:tag</span>
            <span>in:collection</span>
            <span>is:fav</span>
            <span>format:stl</span>
            <span>file:name</span>
            <span>from:site</span>
            <span>by:author</span>
            <span>-exclude</span>
            <span>"quotes for spaces"</span>
          </div>
        </CommandPrimitive>
      </DialogContent>
    </Dialog>
  );
}
