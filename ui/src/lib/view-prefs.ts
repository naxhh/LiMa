/**
 * Sort order, page size and grid density for project grids.
 * The URL wins (bookmarkable views); otherwise the last explicit choice from localStorage.
 */
import * as React from "react";

export type SortOption = { id: string; label: string; sort: string; order: "asc" | "desc" };

const PROJECT_SORTS: SortOption[] = [
  { id: "updated-desc", label: "Recently updated", sort: "updated", order: "desc" },
  { id: "updated-asc", label: "Oldest updated", sort: "updated", order: "asc" },
  { id: "created-desc", label: "Newest added", sort: "created", order: "desc" },
  { id: "created-asc", label: "Oldest added", sort: "created", order: "asc" },
  { id: "modified-desc", label: "Newest files", sort: "modified", order: "desc" },
  { id: "modified-asc", label: "Oldest files", sort: "modified", order: "asc" },
  { id: "name-asc", label: "Name A–Z", sort: "name", order: "asc" },
  { id: "name-desc", label: "Name Z–A", sort: "name", order: "desc" },
  { id: "size-desc", label: "Largest", sort: "size", order: "desc" },
  { id: "size-asc", label: "Smallest", sort: "size", order: "asc" },
  { id: "files-desc", label: "Most files", sort: "files", order: "desc" },
  { id: "files-asc", label: "Fewest files", sort: "files", order: "asc" },
];

/** Library sorts; the first is the default. */
export const LIBRARY_SORTS = PROJECT_SORTS;

/** Collection page sorts: when it was added to the collection, then the library sorts. */
export const COLLECTION_SORTS: SortOption[] = [
  { id: "added-desc", label: "Recently added here", sort: "added", order: "desc" },
  { id: "added-asc", label: "First added here", sort: "added", order: "asc" },
  ...PROJECT_SORTS,
];

export const PAGE_SIZES = [24, 48, 96];
export const DEFAULT_PAGE_SIZE = 48;

type Stored = { sortId?: string; perPage?: number };

function read<T>(key: string): T | null {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "null") as T | null;
  } catch {
    return null;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Not available (private mode etc.): the URL still works.
  }
}

/**
 * The current sort and page size for one kind of page (`storageKey`), from the URL params
 * `sort`, `order`, `per_page`, else the stored choice, else the defaults.
 * `choose*` return the URL changes to apply and remember the choice.
 */
export function useViewPrefs(storageKey: string, options: SortOption[], params: URLSearchParams) {
  const [stored] = React.useState(() => read<Stored>(storageKey) ?? {});
  const fallbackSort = options.find((o) => o.id === stored.sortId) ?? options[0];
  const sortParam = params.get("sort");
  const orderParam = params.get("order");
  const sort =
    options.find((o) => o.sort === sortParam && o.order === orderParam) ?? (sortParam ? options[0] : fallbackSort);
  const perPageParam = Number(params.get("per_page"));
  const storedPerPage = PAGE_SIZES.includes(stored.perPage ?? 0) ? stored.perPage! : DEFAULT_PAGE_SIZE;
  const perPage = PAGE_SIZES.includes(perPageParam) ? perPageParam : storedPerPage;

  const remember = (change: Stored) => write(storageKey, { ...read<Stored>(storageKey), ...change });

  return {
    sort,
    perPage,
    /** URL changes for picking a sort (the default clears the params). */
    chooseSort(id: string): Record<string, string | null> {
      const o = options.find((x) => x.id === id) ?? options[0];
      remember({ sortId: o.id });
      const isDefault = o.id === options[0].id;
      return { sort: isDefault ? null : o.sort, order: isDefault ? null : o.order };
    },
    choosePerPage(n: number): Record<string, string | null> {
      remember({ perPage: n });
      return { per_page: n === DEFAULT_PAGE_SIZE ? null : String(n) };
    },
  };
}

export type Density = "comfortable" | "compact";
const DENSITY_KEY = "lima.grid.density";

/** Card density, shared by every project grid and remembered per browser. */
export function useDensity(): [Density, (d: Density) => void] {
  const [density, setDensity] = React.useState<Density>(() => (read<Density>(DENSITY_KEY) === "compact" ? "compact" : "comfortable"));
  return [
    density,
    (d) => {
      setDensity(d);
      write(DENSITY_KEY, d);
    },
  ];
}

/** Grid classes for a density. */
export function gridClass(density: Density) {
  return density === "compact"
    ? "grid grid-cols-3 gap-2 sm:grid-cols-[repeat(auto-fill,minmax(180px,1fr))] sm:gap-3"
    : "grid grid-cols-2 gap-3 sm:grid-cols-[repeat(auto-fill,minmax(260px,1fr))] sm:gap-5";
}
