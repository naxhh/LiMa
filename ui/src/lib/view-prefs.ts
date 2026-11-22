/**
 * Sort order for project grids.
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

/** Projects loaded per infinite-scroll batch. */
export const PAGE_SIZE = 48;

type Stored = { sortId?: string };

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
 * The current sort for one kind of page (`storageKey`), from the URL params `sort` and
 * `order`, else the stored choice, else the default. `chooseSort` returns the URL changes
 * to apply and remembers the choice.
 */
export function useViewPrefs(storageKey: string, options: SortOption[], params: URLSearchParams) {
  const [stored] = React.useState(() => read<Stored>(storageKey) ?? {});
  const fallbackSort = options.find((o) => o.id === stored.sortId) ?? options[0];
  const sortParam = params.get("sort");
  const orderParam = params.get("order");
  const sort =
    options.find((o) => o.sort === sortParam && o.order === orderParam) ?? (sortParam ? options[0] : fallbackSort);

  const remember = (change: Stored) => write(storageKey, { ...read<Stored>(storageKey), ...change });

  return {
    sort,
    /** URL changes for picking a sort (the default clears the params). */
    chooseSort(id: string): Record<string, string | null> {
      const o = options.find((x) => x.id === id) ?? options[0];
      remember({ sortId: o.id });
      const isDefault = o.id === options[0].id;
      return { sort: isDefault ? null : o.sort, order: isDefault ? null : o.order };
    },
  };
}
