import * as React from "react";
import { Link } from "react-router-dom";
import { Download, Layers, Tag as TagIcon, X } from "lucide-react";

import type { components } from "@/gen/openapi";
import { formatBytes, timeAgo } from "@/lib/format";
import { projectDownloadUrl, thumbUrl } from "@/lib/media";
import { tagTextColor } from "@/lib/tags";
import { cn } from "@/lib/utils";
import { MediaImage } from "@/components/media-image";
import { ModelPlaceholder } from "@/components/model-placeholder";
import { FavouriteButton } from "@/components/favourite-button";
import { TagPickerDialog } from "@/components/tag-picker-dialog";
import { CollectionPickerDialog } from "@/components/collection-picker-dialog";

export type ProjectListItem = components["schemas"]["ProjectListItem"];

const MAX_TAGS = 3;

/** Shown on hover/focus where hovering exists; always visible on touch screens. */
const REVEAL = "[@media(hover:hover)]:opacity-0 group-hover:opacity-100 group-focus-within:opacity-100";

const overlayButton =
  "grid size-8 cursor-pointer place-items-center rounded-full border border-white/15 bg-black/60 text-white backdrop-blur transition-colors hover:bg-black/80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none [&_svg]:size-4";

/**
 * Project tile. Quick actions sit on the image: favourite star (top left), and
 * download all / tags / collections (bottom right). Pass `onRemove` to add a
 * remove button, e.g. "remove from this collection".
 */
export function ProjectCard({
  project: p,
  onRemove,
  removeLabel = "Remove",
  compact = false,
}: {
  project: ProjectListItem;
  onRemove?: () => void;
  removeLabel?: string;
  /** Smaller card for dense grids: name and size only. */
  compact?: boolean;
}) {
  const [picker, setPicker] = React.useState<"tags" | "collections" | null>(null);
  const extraTags = p.tags.length - MAX_TAGS;
  // Projects without images show a preview of one of their models.
  const cover = p.main_image_id ?? p.preview_id;

  return (
    <div className="group relative transition-transform hover:-translate-y-0.5">
      <Link
        to={`/projects/${p.id}`}
        className="flex h-full flex-col overflow-hidden rounded-[14px] border bg-card text-card-foreground transition-colors group-hover:border-line-strong focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        <div className="corners relative aspect-[4/3] overflow-hidden border-b bg-thumb">
          {cover ? (
            <MediaImage
              src={thumbUrl(p.id, cover)}
              alt=""
              className="transition-transform duration-300 group-hover:scale-[1.03]"
            />
          ) : (
            <ModelPlaceholder />
          )}
          {p.model_formats.length > 0 ? (
            <div className={cn("absolute top-3 right-3 z-[3] gap-1", compact ? "hidden sm:flex" : "flex")}>
              {p.model_formats.slice(0, 3).map((f) => (
                <span
                  key={f}
                  className="rounded-[5px] border border-white/15 bg-black/70 px-1.5 py-0.5 font-mono text-[10px] font-medium tracking-wide text-white"
                >
                  {f}
                </span>
              ))}
            </div>
          ) : null}
          <span
            className={cn(
              "absolute bottom-2.5 left-3 z-[3] hidden rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] text-white/80",
              !compact && "sm:inline",
            )}
          >
            {p.image_count > 0 ? `${p.image_count} image${p.image_count === 1 ? "" : "s"}` : cover ? "model preview" : "no image"}
          </span>
        </div>
        <div className={cn("flex flex-1 flex-col", compact ? "gap-1.5 px-2.5 pt-2 pb-2.5 sm:px-3" : "gap-2.5 px-4 pt-3.5 pb-4")}>
          <h2
            className={cn(
              "font-display line-clamp-2 leading-tight font-semibold tracking-tight",
              compact ? "text-[13px] sm:text-sm" : "text-[17px]",
            )}
          >
            {p.name}
          </h2>
          {!compact && p.tags.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {p.tags.slice(0, MAX_TAGS).map((t) => (
                <span
                  key={t.id}
                  className="max-w-full truncate rounded-full px-2 py-0.5 text-[11px] font-medium"
                  style={{ backgroundColor: t.color, color: tagTextColor(t.color) }}
                >
                  {t.name}
                </span>
              ))}
              {extraTags > 0 ? (
                <span className="rounded-full border px-2 py-0.5 text-[11px] text-muted-foreground">+{extraTags}</span>
              ) : null}
            </div>
          ) : null}
          {compact ? (
            <div className="mt-auto truncate font-mono text-[10px] text-muted-foreground sm:text-[11px]">{formatBytes(p.total_bytes)}</div>
          ) : (
            <div className="mt-auto flex justify-between gap-2 border-t border-dashed border-line-strong pt-2.5 font-mono text-[11px] text-muted-foreground">
              <span>
                {p.asset_count} file{p.asset_count === 1 ? "" : "s"} · {formatBytes(p.total_bytes)}
              </span>
              <span>{timeAgo(p.updated_at)}</span>
            </div>
          )}
        </div>
      </Link>

      {/* Quick actions, positioned over the image (same 4:3 box as the thumbnail). */}
      <div className="pointer-events-none absolute inset-x-0 top-0 z-10 aspect-[4/3]">
        <div className={cn("pointer-events-auto absolute top-2.5 left-2.5 transition-opacity", !p.is_favourite && REVEAL)}>
          <FavouriteButton projectId={p.id} isFavourite={p.is_favourite} variant="overlay" />
        </div>
        <div
          className={cn(
            "pointer-events-auto absolute right-2.5 bottom-2.5 gap-1.5 transition-opacity",
            compact ? "hidden sm:flex" : "flex",
            REVEAL,
          )}
        >
          {onRemove ? (
            <button type="button" onClick={onRemove} className={overlayButton} aria-label={`${removeLabel}: ${p.name}`} title={removeLabel}>
              <X />
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => setPicker("collections")}
            className={overlayButton}
            aria-label={`Collections for ${p.name}`}
            title="Add to or remove from collections"
          >
            <Layers />
          </button>
          <button
            type="button"
            onClick={() => setPicker("tags")}
            className={overlayButton}
            aria-label={`Tags for ${p.name}`}
            title="Add or remove tags"
          >
            <TagIcon />
          </button>
          {p.asset_count > 0 ? (
            <a
              href={projectDownloadUrl(p.id)}
              download
              className={overlayButton}
              aria-label={`Download all files of ${p.name}`}
              title="Download all (zip)"
            >
              <Download />
            </a>
          ) : null}
        </div>
      </div>

      {picker === "tags" ? (
        <TagPickerDialog projectId={p.id} tags={p.tags} open onOpenChange={(o) => !o && setPicker(null)} />
      ) : null}
      {picker === "collections" ? (
        <CollectionPickerDialog
          projectId={p.id}
          collections={p.collections}
          open
          onOpenChange={(o) => !o && setPicker(null)}
        />
      ) : null}
    </div>
  );
}

export function ProjectCardSkeleton() {
  return (
    <div className="overflow-hidden rounded-[14px] border bg-card" aria-hidden>
      <div className="bg-dots aspect-[4/3] animate-pulse bg-thumb" />
      <div className="space-y-3 p-4">
        <div className="h-4 w-2/3 rounded bg-muted" />
        <div className="h-3 w-1/3 rounded bg-muted" />
      </div>
    </div>
  );
}
