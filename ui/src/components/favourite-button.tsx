import { Star } from "lucide-react";

import { useFavourite } from "@/lib/project-mutations";
import { cn } from "@/lib/utils";

/** Star toggle. `variant="overlay"` is the dark round button used on top of images. */
export function FavouriteButton({
  projectId,
  isFavourite,
  variant = "outline",
  className,
}: {
  projectId: string;
  isFavourite: boolean;
  variant?: "outline" | "overlay";
  className?: string;
}) {
  const { on, toggle } = useFavourite(projectId, isFavourite);
  return (
    <button
      type="button"
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        toggle();
      }}
      aria-pressed={on}
      aria-label={on ? "Remove from favourites" : "Add to favourites"}
      title={on ? "Remove from favourites" : "Add to favourites"}
      className={cn(
        "grid shrink-0 cursor-pointer place-items-center transition-colors",
        variant === "overlay"
          ? "size-8 rounded-full border border-white/15 bg-black/60 text-white backdrop-blur hover:bg-black/80"
          : "size-10 rounded-lg border border-input bg-card text-muted-foreground hover:bg-accent hover:text-foreground",
        on && "text-[#FFB547] hover:text-[#FFB547]",
        className,
      )}
    >
      <Star className={cn("size-4", on && "fill-current")} />
    </button>
  );
}
