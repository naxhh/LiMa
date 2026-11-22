import type { Collection } from "@/lib/collections";
import { thumbUrl } from "@/lib/media";
import { cn } from "@/lib/utils";
import { MediaImage } from "@/components/media-image";
import { ModelPlaceholder } from "@/components/model-placeholder";

/** 2×2 grid of the collection's latest project covers; fills empty slots with the dot pattern. */
export function CollectionMosaic({ covers, className }: { covers: Collection["covers"]; className?: string }) {
  if (covers.length === 0) {
    return (
      <div className={cn("relative bg-thumb", className)}>
        <ModelPlaceholder />
      </div>
    );
  }
  if (covers.length === 1) {
    return (
      <div className={cn("relative overflow-hidden bg-thumb", className)}>
        <MediaImage src={thumbUrl(covers[0].project_id, covers[0].asset_id)} alt="" />
      </div>
    );
  }
  const slots = [0, 1, 2, 3].map((i) => covers[i] ?? null);
  return (
    <div className={cn("grid grid-cols-2 grid-rows-2 gap-px bg-border", className)}>
      {slots.map((c, i) => (
        <div key={i} className="bg-dots relative overflow-hidden bg-thumb">
          {c ? <MediaImage src={thumbUrl(c.project_id, c.asset_id)} alt="" /> : null}
        </div>
      ))}
    </div>
  );
}
