import * as React from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { MoreHorizontal, Pencil, Star, Trash2 } from "lucide-react";

import { apiNoBody } from "@/lib/api";
import { COLLECTIONS_QUERY_KEY, invalidateCollections, useCollection, type Collection, type CollectionDetail } from "@/lib/collections";
import { PROJECT_CACHE_KEYS, optimisticHandlers, patchProjects } from "@/lib/optimistic";
import { formatBytes } from "@/lib/format";
import { apiErrorMessage } from "@/lib/tags";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PageBody } from "@/components/layout";
import { ProjectCard, ProjectCardSkeleton } from "@/components/project-card";
import { ViewControls } from "@/components/view-controls";
import { COLLECTION_SORTS, gridClass, useDensity, useViewPrefs } from "@/lib/view-prefs";
import { CollectionFormDialog } from "@/components/collection-form-dialog";

export function CollectionDetailPage() {
  const { collectionId } = useParams();
  if (!collectionId) return <PageBody className="py-10">Missing collection id</PageBody>;
  return <CollectionDetailInner id={collectionId} />;
}

function CollectionDetailInner({ id }: { id: string }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const view = useViewPrefs("lima.collection.view", COLLECTION_SORTS, searchParams);
  const [density, setDensity] = useDensity();
  const collectionQ = useCollection(id, view.sort.id === COLLECTION_SORTS[0].id ? undefined : view.sort);
  const c = collectionQ.data;
  const setParams = (changes: Record<string, string | null>) =>
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(changes)) {
          if (value) next.set(key, value);
          else next.delete(key);
        }
        return next;
      },
      { replace: true },
    );

  const [editing, setEditing] = React.useState(false);
  const [confirmDelete, setConfirmDelete] = React.useState(false);

  const removeM = useMutation({
    mutationFn: (projectId: string) => apiNoBody("DELETE", `/collections/${id}/projects/${projectId}`),
    ...optimisticHandlers<string>(qc, {
      mutationKey: ["collection-edit", id],
      keys: [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY],
      apply: (projectId) => {
        // The card leaves this page; the project no longer lists this collection.
        qc.setQueriesData<CollectionDetail>({ queryKey: ["collection", id] }, (d) =>
          d ? { ...d, projects: d.projects.filter((p) => p.id !== projectId), project_count: Math.max(0, d.project_count - 1) } : d,
        );
        patchProjects(
          qc,
          (p) => ({
            ...p,
            collections: p.collections.filter((m) => m.id !== id),
            ...(c?.is_favourites ? { is_favourite: false } : {}),
          }),
          projectId,
        );
        qc.setQueryData<Collection[]>(COLLECTIONS_QUERY_KEY, (list) =>
          list?.map((x) => (x.id === id ? { ...x, project_count: Math.max(0, x.project_count - 1) } : x)),
        );
      },
      errorTitle: `Couldn't remove from ${c?.name ?? "the collection"}`,
      invalidate: [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY],
    }),
  });
  const deleteM = useMutation({
    mutationFn: () => apiNoBody("DELETE", `/collections/${id}`),
    onSuccess: async () => {
      qc.removeQueries({ queryKey: ["collection", id] });
      await Promise.all([invalidateCollections(qc), qc.invalidateQueries({ queryKey: ["project"] })]);
      navigate("/collections");
    },
  });

  if (collectionQ.isError) {
    return (
      <PageBody className="py-10">
        <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive">
          Could not load the collection: {apiErrorMessage(collectionQ.error)}
        </div>
        <Link to="/collections" className="mt-4 inline-block text-sm text-signal">
          ← All collections
        </Link>
      </PageBody>
    );
  }

  const totalBytes = c?.projects.reduce((n, p) => n + p.total_bytes, 0) ?? 0;
  const totalFiles = c?.projects.reduce((n, p) => n + p.asset_count, 0) ?? 0;

  return (
    <div>
      <section className="bg-grid border-b">
        <div className="mx-auto flex max-w-[1440px] flex-col gap-5 px-4 pt-5 pb-8 sm:px-8">
          <nav aria-label="Breadcrumb" className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
            <Link to="/collections" className="hover:text-signal">
              collections
            </Link>
            <span className="text-dim">/</span>
            <span className="truncate text-foreground">{c?.name ?? "…"}</span>
          </nav>
          <div className="flex flex-wrap items-end justify-between gap-6">
            <div className="flex min-w-0 max-w-3xl flex-col gap-2.5">
              <span className="flex items-center gap-1.5 font-mono text-xs tracking-[0.12em] text-signal">
                {c?.is_favourites ? (
                  <>
                    <Star className="size-3.5 fill-[#FFB547] text-[#FFB547]" /> // FAVOURITES
                  </>
                ) : (
                  "// COLLECTION"
                )}
              </span>
              <h1 className="font-display text-4xl leading-none font-semibold tracking-[-0.03em] break-words sm:text-[52px]">
                {c?.name ?? " "}
              </h1>
              {c?.description ? (
                <p className="text-[15px] leading-relaxed whitespace-pre-line text-muted-foreground">{c.description}</p>
              ) : null}
              {c ? (
                <span className="font-mono text-[13px] text-muted-foreground">
                  {c.project_count} project{c.project_count === 1 ? "" : "s"} · {totalFiles} files · {formatBytes(totalBytes)}
                </span>
              ) : null}
            </div>
            {c ? (
              <div className="flex gap-2">
                <Button variant="outline" onClick={() => setEditing(true)}>
                  <Pencil />
                  Edit
                </Button>
                {c.is_favourites ? null : (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="outline" size="icon" aria-label="More actions">
                      <MoreHorizontal />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem variant="destructive" onClick={() => setConfirmDelete(true)}>
                      <Trash2 />
                      Delete collection
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
                )}
              </div>
            ) : null}
          </div>
        </div>
      </section>

      <PageBody className="pt-7 pb-16">
        {c && c.projects.length === 0 ? (
          <div className="bg-dots rounded-2xl border border-dashed p-10 text-center">
            <div className="font-display text-xl font-semibold">Nothing here yet</div>
            <p className="mt-2 text-sm text-muted-foreground">
              {c.is_favourites ? (
                <>
                  Star a project (<Star className="inline size-3.5 align-[-2px]" /> on its card or page) to add it here.
                </>
              ) : (
                <>
                  Open a project and use <span className="text-foreground">Add to collection</span>, or the{" "}
                  collections button on a card, to put it here.
                </>
              )}
            </p>
            <Button asChild variant="outline" className="mt-5">
              <Link to="/">Browse the library</Link>
            </Button>
          </div>
        ) : null}

        {c && c.projects.length > 0 ? (
          <div className="mb-5 flex flex-wrap items-center justify-end gap-3">
            <span className="mr-auto font-mono text-xs text-muted-foreground">
              {c.projects.length} project{c.projects.length === 1 ? "" : "s"}
            </span>
            <ViewControls
              options={COLLECTION_SORTS}
              sort={view.sort}
              onSort={(sortId) => setParams(view.chooseSort(sortId))}
              density={density}
              onDensity={setDensity}
            />
          </div>
        ) : null}

        <div className={gridClass(density)}>
          {!c
            ? Array.from({ length: 4 }, (_, i) => <ProjectCardSkeleton key={i} />)
            : c.projects.map((p) => (
                <ProjectCard
                  key={p.id}
                  project={p}
                  onRemove={() => removeM.mutate(p.id)}
                  removeLabel={c.is_favourites ? "Remove from favourites" : "Remove from this collection"}
                  compact={density === "compact"}
                />
              ))}
        </div>
      </PageBody>

      {c ? <CollectionFormDialog collection={c} open={editing} onOpenChange={setEditing} /> : null}

      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete “{c?.name}”?</DialogTitle>
            <DialogDescription>
              Only the collection is deleted. Its {c?.project_count ?? 0} project{c?.project_count === 1 ? "" : "s"} and their
              files stay in the library.
            </DialogDescription>
          </DialogHeader>
          {deleteM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(deleteM.error)}</div> : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDelete(false)}>
              Cancel
            </Button>
            <Button variant="destructive" disabled={deleteM.isPending} onClick={() => deleteM.mutate()}>
              Delete collection
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
