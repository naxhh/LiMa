import type { SortOption } from "@/lib/view-prefs";

const SELECT_CLASS =
  "h-8 rounded-lg border border-input bg-background px-2 font-mono text-xs text-muted-foreground uppercase hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none disabled:opacity-60";

/** Sort select for a project grid. */
export function SortSelect({
  options,
  sort,
  onSort,
  searching = false,
}: {
  options: SortOption[];
  sort: SortOption;
  onSort: (id: string) => void;
  /** Search results are ordered by relevance: the sort is shown as "Best match" and disabled. */
  searching?: boolean;
}) {
  return (
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
  );
}
