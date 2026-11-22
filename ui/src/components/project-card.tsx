import { Link } from "react-router-dom";

import type { components } from "@/gen/openapi";
import { formatBytes, timeAgo } from "@/lib/format";
import { thumbUrl } from "@/lib/media";
import { tagTextColor } from "@/lib/tags";
import { MediaImage } from "@/components/media-image";
import { ModelPlaceholder } from "@/components/model-placeholder";

export type ProjectListItem = components["schemas"]["ProjectListItem"];

const MAX_TAGS = 3;

export function ProjectCard({ project: p }: { project: ProjectListItem }) {
  const extraTags = p.tags.length - MAX_TAGS;
  return (
    <Link
      to={`/projects/${p.id}`}
      className="group flex flex-col overflow-hidden rounded-[14px] border bg-card text-card-foreground transition hover:-translate-y-0.5 hover:border-line-strong focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
    >
      <div className="corners relative aspect-[4/3] overflow-hidden border-b bg-thumb">
        {p.main_image_id ? (
          <MediaImage
            src={thumbUrl(p.id, p.main_image_id)}
            alt=""
            className="transition-transform duration-300 group-hover:scale-[1.03]"
          />
        ) : (
          <ModelPlaceholder />
        )}
        {p.model_formats.length > 0 ? (
          <div className="absolute top-3 right-3 z-[3] flex gap-1">
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
        <span className="absolute bottom-2.5 left-3 z-[3] rounded bg-black/60 px-1.5 py-0.5 font-mono text-[10px] text-white/80">
          {p.image_count === 0 ? "no image" : `${p.image_count} image${p.image_count === 1 ? "" : "s"}`}
        </span>
      </div>
      <div className="flex flex-1 flex-col gap-2.5 px-4 pt-3.5 pb-4">
        <h2 className="font-display line-clamp-2 text-[17px] leading-tight font-semibold tracking-tight">{p.name}</h2>
        {p.tags.length > 0 ? (
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
        <div className="mt-auto flex justify-between gap-2 border-t border-dashed border-line-strong pt-2.5 font-mono text-[11px] text-muted-foreground">
          <span>
            {p.asset_count} file{p.asset_count === 1 ? "" : "s"} · {formatBytes(p.total_bytes)}
          </span>
          <span>{timeAgo(p.updated_at)}</span>
        </div>
      </div>
    </Link>
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
