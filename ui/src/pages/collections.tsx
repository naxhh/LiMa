import * as React from "react";
import { Link, useNavigate } from "react-router-dom";
import { Plus, Star } from "lucide-react";

import { useCollections } from "@/lib/collections";
import { timeAgo } from "@/lib/format";
import { apiErrorMessage } from "@/lib/tags";
import { Button } from "@/components/ui/button";
import { PageBody, PageHero } from "@/components/layout";
import { CollectionMosaic } from "@/components/collection-mosaic";
import { CollectionFormDialog } from "@/components/collection-form-dialog";

export function CollectionsPage() {
  const collectionsQ = useCollections();
  const navigate = useNavigate();
  const [creating, setCreating] = React.useState(false);
  const collections = collectionsQ.data ?? [];

  return (
    <div>
      <PageHero
        index="03"
        label="Collections"
        title="Collections"
        subtitle={`${collections.length} collection${collections.length === 1 ? "" : "s"} · hand-picked groups of projects`}
      >
        <Button onClick={() => setCreating(true)}>
          <Plus strokeWidth={2.4} />
          New collection
        </Button>
      </PageHero>

      <PageBody className="pt-7 pb-16">
        {collectionsQ.isError ? (
          <div className="text-sm text-destructive">Failed to load collections: {apiErrorMessage(collectionsQ.error)}</div>
        ) : null}

        {!collectionsQ.isLoading && !collectionsQ.isError && collections.length === 0 ? (
          <div className="bg-dots rounded-2xl border border-dashed p-10 text-center">
            <div className="font-display text-xl font-semibold">No collections yet</div>
            <p className="mt-2 text-sm text-muted-foreground">
              Group projects however you like: a gift list, printer upgrades, things to print next.
            </p>
            <Button className="mt-5" onClick={() => setCreating(true)}>
              <Plus strokeWidth={2.4} />
              Create your first collection
            </Button>
          </div>
        ) : null}

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-[repeat(auto-fill,minmax(280px,1fr))] sm:gap-5">
          {collectionsQ.isLoading
            ? Array.from({ length: 4 }, (_, i) => (
                <div key={i} className="bg-dots aspect-[4/3] animate-pulse rounded-[14px] border bg-card" aria-hidden />
              ))
            : collections.map((c) => (
                <Link
                  key={c.id}
                  to={`/collections/${c.id}`}
                  className="group flex flex-col overflow-hidden rounded-[14px] border bg-card transition hover:-translate-y-0.5 hover:border-line-strong focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <div className="corners relative border-b">
                    <CollectionMosaic covers={c.covers} className="aspect-[4/3]" />
                    <span className="absolute bottom-2.5 left-3 z-[3] rounded bg-black/65 px-1.5 py-0.5 font-mono text-[10px] text-white/85">
                      {c.project_count} project{c.project_count === 1 ? "" : "s"}
                    </span>
                  </div>
                  <div className="flex flex-1 flex-col gap-1.5 px-4 pt-3.5 pb-4">
                    <h2 className="font-display line-clamp-2 flex items-center gap-1.5 text-[17px] leading-tight font-semibold tracking-tight">
                      {c.is_favourites ? <Star className="size-4 shrink-0 fill-[#FFB547] text-[#FFB547]" aria-hidden /> : null}
                      {c.name}
                    </h2>
                    {c.description || c.is_favourites ? (
                      <p className="line-clamp-2 text-sm text-muted-foreground">
                        {c.description || "Projects you starred"}
                      </p>
                    ) : null}
                    <span className="mt-auto pt-2 font-mono text-[11px] text-dim">updated {timeAgo(c.updated_at)}</span>
                  </div>
                </Link>
              ))}
        </div>
      </PageBody>

      <CollectionFormDialog open={creating} onOpenChange={setCreating} onSaved={(c) => navigate(`/collections/${c.id}`)} />
    </div>
  );
}
