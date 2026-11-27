import * as React from "react";
import { useMutation, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";

import { apiJson } from "@/lib/api";
import { TAGS_QUERY_KEY, apiErrorMessage, useAllTags, type Tag } from "@/lib/tags";
import { useLibraryStats } from "@/lib/sync";
import { PROJECT_CACHE_KEYS } from "@/lib/optimistic";
import { PageBody, PageHero } from "@/components/layout";
import { AdminNav } from "@/components/admin-nav";
import { adminIndex } from "@/lib/admin";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TagBadge } from "@/components/tag-badge";
import {
  CommandDialog,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

/** Refreshes everything a tag merge or delete can change. */
function invalidateTagChanges(qc: QueryClient) {
  return Promise.all([...PROJECT_CACHE_KEYS, TAGS_QUERY_KEY].map((key) => qc.invalidateQueries({ queryKey: key })));
}

export function AdminTagsPage() {
  const qc = useQueryClient();
  const tagsQ = useAllTags();
  const statsQ = useLibraryStats();

  const tags = tagsQ.data ?? [];
  const totalProjects = statsQ.data?.projects ?? 0;

  const [maxCount, setMaxCount] = React.useState(1);
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [merging, setMerging] = React.useState<Tag | null>(null);
  const [confirmingDelete, setConfirmingDelete] = React.useState(false);

  const unused = tags.filter((t) => t.project_count === 0).length;

  const visible = tags
    .filter((t) => t.project_count <= maxCount)
    .sort((a, b) => a.project_count - b.project_count || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));

  const bulkDeleteM = useMutation({
    mutationFn: (ids: string[]) => apiJson<{ deleted: number }>("POST", "/tags/delete", { tag_ids: ids }),
    onSuccess: (res) => {
      toast.success(`${res.deleted} tag${res.deleted === 1 ? "" : "s"} deleted`);
      setSelected(new Set());
      void invalidateTagChanges(qc);
    },
    onError: (err) => toast.error("Couldn't delete tags", { description: apiErrorMessage(err) }),
  });

  const mergeM = useMutation({
    mutationFn: ({ from, into }: { from: Tag; into: Tag }) =>
      apiJson<{ projects_affected: number }>("POST", `/tags/${from.id}/merge`, { into_tag_id: into.id }),
    onSuccess: (res, { from, into }) => {
      toast.success(`Merged “${from.name}” into “${into.name}” · ${res.projects_affected} project${res.projects_affected === 1 ? "" : "s"}`);
      setSelected(new Set());
      void invalidateTagChanges(qc);
    },
    onError: (err) => toast.error("Couldn't merge tags", { description: apiErrorMessage(err) }),
  });

  const toggleOne = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const allSelected = visible.length > 0 && visible.every((t) => selected.has(t.id));
  const selectAllRef = React.useRef<HTMLInputElement>(null);
  React.useEffect(() => {
    if (selectAllRef.current) selectAllRef.current.indeterminate = selected.size > 0 && !allSelected;
  }, [selected, allSelected]);

  const selectedTags = tags.filter((t) => selected.has(t.id));
  const selectedProjects = selectedTags.reduce((n, t) => n + t.project_count, 0);

  return (
    <div>
      <PageHero
        index={adminIndex("/admin/tags")}
        label="Tag cleanup"
        title="Tag cleanup"
        subtitle={`${tags.length} tag${tags.length === 1 ? "" : "s"} · ${unused} unused · remove tags nobody uses, or fold rare ones into another`}
      />

      <PageBody className="flex flex-col gap-6 pt-7 pb-16">
        <AdminNav />
        {tagsQ.isError ? <div className="text-sm text-destructive">{apiErrorMessage(tagsQ.error)}</div> : null}

        <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
          <label className="flex items-center gap-3 text-sm text-muted-foreground">
            <span>Used by at most</span>
            <input
              type="range"
              min={0}
              max={10}
              step={1}
              value={maxCount}
              onChange={(e) => {
                setMaxCount(Number(e.target.value));
                setSelected(new Set());
              }}
              className="w-40 accent-[var(--signal)]"
            />
            <span className="font-mono text-foreground">
              {maxCount} project{maxCount === 1 ? "" : "s"}
            </span>
          </label>
          <span className="font-mono text-xs text-dim">
            {visible.length} tag{visible.length === 1 ? "" : "s"} match
          </span>
        </div>

        {tagsQ.isLoading ? (
          <div className="text-sm text-muted-foreground">Loading…</div>
        ) : visible.length === 0 ? (
          <div className="font-mono text-sm text-dim">No tags used by {maxCount} or fewer projects.</div>
        ) : (
          <section className="overflow-hidden rounded-xl border bg-card">
            <header className="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
              <label className="flex items-center gap-2 text-sm text-muted-foreground">
                <input
                  ref={selectAllRef}
                  type="checkbox"
                  checked={allSelected}
                  onChange={() => {
                    const next = new Set<string>();
                    if (!allSelected) for (const t of visible) next.add(t.id);
                    setSelected(next);
                  }}
                  className="accent-[var(--signal)]"
                />
                Select all
              </label>
              {selected.size > 0 ? (
                <Button
                  variant="outline"
                  size="sm"
                  className="border-destructive/40 text-destructive hover:bg-destructive/10"
                  disabled={bulkDeleteM.isPending}
                  onClick={() => setConfirmingDelete(true)}
                >
                  <Trash2 />
                  Delete {selected.size} tag{selected.size === 1 ? "" : "s"}
                </Button>
              ) : null}
            </header>
            <ul>
              {visible.map((t) => {
                const pct = totalProjects > 0 ? ((t.project_count / totalProjects) * 100).toFixed(1) : "0.0";
                return (
                  <li key={t.id} className="flex flex-wrap items-center gap-3 border-b px-4 py-3 last:border-b-0">
                    <input
                      type="checkbox"
                      checked={selected.has(t.id)}
                      onChange={() => toggleOne(t.id)}
                      aria-label={`Select ${t.name}`}
                      className="accent-[var(--signal)]"
                    />
                    <TagBadge name={t.name} color={t.color} />
                    <span className="font-mono text-xs text-muted-foreground">
                      {t.project_count === 0 ? (
                        <span className="text-warn">unused</span>
                      ) : (
                        <>
                          {t.project_count} project{t.project_count === 1 ? "" : "s"}
                          <span className="text-dim"> · {pct}% of library</span>
                        </>
                      )}
                    </span>
                    <div className="ml-auto flex gap-1">
                      <Button variant="ghost" size="sm" onClick={() => setMerging(t)}>
                        Merge into…
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        className="text-destructive hover:text-destructive"
                        aria-label={`Delete ${t.name}`}
                        title="Delete"
                        disabled={bulkDeleteM.isPending}
                        onClick={() => bulkDeleteM.mutate([t.id])}
                      >
                        <Trash2 />
                      </Button>
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        )}
      </PageBody>

      <MergeTagDialog
        tag={merging}
        tags={tags}
        onOpenChange={(o) => !o && setMerging(null)}
        onMerge={(into) => {
          if (merging) mergeM.mutate({ from: merging, into });
          setMerging(null);
        }}
      />

      <Dialog open={confirmingDelete} onOpenChange={setConfirmingDelete}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {selected.size} tag{selected.size === 1 ? "" : "s"}?</DialogTitle>
            <DialogDescription>
              {selected.size} tag{selected.size === 1 ? "" : "s"} will be removed from {selectedProjects} project{selectedProjects === 1 ? "" : "s"}. Projects are not deleted.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmingDelete(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={bulkDeleteM.isPending}
              onClick={() => {
                bulkDeleteM.mutate([...selected]);
                setConfirmingDelete(false);
              }}
            >
              Delete {selected.size} tag{selected.size === 1 ? "" : "s"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Pick the tag to fold the source tag into, from every other tag in the library. */
function MergeTagDialog({
  tag,
  tags,
  onOpenChange,
  onMerge,
}: {
  tag: Tag | null;
  tags: Tag[];
  onOpenChange: (open: boolean) => void;
  onMerge: (into: Tag) => void;
}) {
  const [search, setSearch] = React.useState("");
  const targets = tags
    .filter((t) => t.id !== tag?.id)
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));

  return (
    <CommandDialog
      open={tag !== null}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) setSearch("");
      }}
      title="Merge into"
      description={`Pick the tag to merge “${tag?.name ?? ""}” into`}
    >
      <CommandInput placeholder="Search tags…" value={search} onValueChange={setSearch} />
      <CommandList>
        <CommandEmpty>No matching tags.</CommandEmpty>
        {targets.map((t) => (
          <CommandItem key={t.id} value={t.name} onSelect={() => onMerge(t)}>
            <span className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: t.color }} />
            <span className="flex-1 truncate">{t.name}</span>
            <span className="text-xs text-muted-foreground">{t.project_count}</span>
          </CommandItem>
        ))}
      </CommandList>
    </CommandDialog>
  );
}
