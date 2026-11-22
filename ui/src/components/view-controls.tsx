import { Grid2x2, Grid3x3 } from "lucide-react";

import type { Density, SortOption } from "@/lib/view-prefs";
import { PAGE_SIZES } from "@/lib/view-prefs";
import { cn } from "@/lib/utils";

const SELECT_CLASS =
  "h-8 rounded-lg border border-input bg-background px-2 font-mono text-xs text-muted-foreground uppercase hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none disabled:opacity-60";

/** Sort, optional page size and grid density controls for a project grid. */
export function ViewControls({
  options,
  sort,
  onSort,
  searching = false,
  perPage,
  onPerPage,
  density,
  onDensity,
}: {
  options: SortOption[];
  sort: SortOption;
  onSort: (id: string) => void;
  /** Search results are ordered by relevance: the sort is shown as "Best match" and disabled. */
  searching?: boolean;
  perPage?: number;
  onPerPage?: (n: number) => void;
  density: Density;
  onDensity: (d: Density) => void;
}) {
  return (
    <>
      <select
        aria-label="Sort projects"
        value={searching ? "relevance" : sort.id}
        disabled={searching}
        onChange={(e) => onSort(e.target.value)}
        className={SELECT_CLASS}
      >
        {searching ? (
          <option value="relevance">Best match</option>
        ) : (
          options.map((o) => (
            <option key={o.id} value={o.id}>
              {o.label}
            </option>
          ))
        )}
      </select>
      {perPage !== undefined && onPerPage ? (
        <select aria-label="Projects per page" value={perPage} onChange={(e) => onPerPage(Number(e.target.value))} className={SELECT_CLASS}>
          {PAGE_SIZES.map((n) => (
            <option key={n} value={n}>
              {n} / page
            </option>
          ))}
        </select>
      ) : null}
      <div role="group" aria-label="Card size" className="flex h-8 items-center rounded-lg border border-input bg-background p-0.5">
        {(
          [
            ["comfortable", Grid2x2, "Large cards"],
            ["compact", Grid3x3, "Compact cards"],
          ] as const
        ).map(([value, Icon, label]) => (
          <button
            key={value}
            type="button"
            aria-label={label}
            title={label}
            aria-pressed={density === value}
            onClick={() => onDensity(value)}
            className={cn(
              "grid h-full w-7 place-items-center rounded-md text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
              density === value && "bg-secondary text-foreground",
            )}
          >
            <Icon className="size-3.5" />
          </button>
        ))}
      </div>
    </>
  );
}
