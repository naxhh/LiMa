import * as React from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";

import type { components } from "@/gen/openapi";
import { downloadProjects, useBulkAction, type BulkAction } from "@/lib/bulk";
import { rememberList } from "@/lib/list-nav";
import { projectDownloadUrl } from "@/lib/media";
import type { Selection } from "@/lib/selection";
import { downloadUrl, useShortcuts, type ShortcutMap } from "@/lib/shortcuts";
import { useTrash } from "@/lib/trash";

type ProjectListItem = components["schemas"]["ProjectListItem"];

export type GridDialog =
  | { kind: "tags" | "collections" | "delete"; scope: "selection" }
  | { kind: "tags" | "collections" | "delete"; scope: "project"; id: string };

/** Columns the grid actually renders right now. */
function columns(grid: HTMLElement | null): number {
  if (!grid) return 1;
  const cols = getComputedStyle(grid).gridTemplateColumns.split(" ").filter(Boolean).length;
  return Math.max(1, cols);
}

/** The first card at least partly below the sticky header, so the first key starts where the user is looking. */
function firstVisible(grid: HTMLElement | null): number {
  if (!grid) return 0;
  const top = document.querySelector("header")?.getBoundingClientRect().bottom ?? 0;
  const i = Array.from(grid.children).findIndex((el) => el.getBoundingClientRect().bottom > top + 24);
  return Math.max(0, i);
}

/**
 * Keyboard control for a grid of project cards: a focused card moved with arrows / h j k l,
 * Enter or o to open, x to select, and f d t a # Delete acting on the selection (when there
 * is one) or the focused card. The focus resets whenever `listKey` (query, sort…) changes.
 */
export function useGridKeys({
  items,
  listKey,
  gridRef,
  selection,
  onSelectAll,
  hasNextPage = false,
  fetchNextPage,
}: {
  items: ProjectListItem[];
  listKey: string;
  gridRef: React.RefObject<HTMLElement | null>;
  selection: Selection;
  /** Selects every match (defaults to the loaded cards). */
  onSelectAll?: () => void;
  hasNextPage?: boolean;
  fetchNextPage?: () => unknown;
}) {
  const navigate = useNavigate();
  const location = useLocation();
  const [focus, setFocus] = React.useState<{ key: string; index: number } | null>(null);
  const [dialog, setDialog] = React.useState<GridDialog | null>(null);
  const bulkSelection = useBulkAction(selection.exit);
  const bulkOne = useBulkAction();
  const trashDays = useTrash().data?.retention_days ?? 0;

  // Past the loaded end the index waits for the next page; until then the last card shows it.
  const index = focus && focus.key === listKey && items.length > 0 ? Math.min(focus.index, items.length - 1) : null;
  const focused = index === null ? null : items[index];

  React.useEffect(() => {
    if (index === null) return;
    gridRef.current?.children[index]?.scrollIntoView({ block: "nearest" });
  }, [index, gridRef]);

  const listUrl = location.pathname + location.search;
  /** Remembers this list for the project page's `u` / `j` / `k`. Call before opening a card. */
  const remember = () => rememberList(listUrl, items.map((p) => p.id));
  const open = (id: string) => {
    remember();
    navigate(`/projects/${id}`);
  };

  const move = (delta: "left" | "right" | "up" | "down") => () => {
    if (items.length === 0) return;
    if (index === null) return setFocus({ key: listKey, index: Math.min(firstVisible(gridRef.current), items.length - 1) });
    const step = delta === "left" ? -1 : delta === "right" ? 1 : delta === "up" ? -columns(gridRef.current) : columns(gridRef.current);
    const next = index + step;
    if (next < 0) return;
    if (next >= items.length) {
      if (hasNextPage) {
        fetchNextPage?.();
        return setFocus({ key: listKey, index: next });
      }
      // Down from a row above the last one lands on the last card.
      if (delta === "down" && index < items.length - 1) setFocus({ key: listKey, index: items.length - 1 });
      return;
    }
    setFocus({ key: listKey, index: next });
  };

  const many = selection.active && selection.ids.length > 0;
  /** The bulk-bar action on the selection, else the same thing for the focused card. */
  const act = (kind: "favourite" | "download" | GridDialog["kind"]) => () => {
    if (!many && !focused) return false;
    if (kind === "download") {
      if (many) return downloadProjects(selection.ids);
      if (focused!.asset_count === 0) return void toast("Nothing to download");
      return downloadUrl(projectDownloadUrl(focused!.id));
    }
    if (kind === "favourite") {
      const chosen = new Set(selection.ids);
      const targets = many ? items.filter((p) => chosen.has(p.id)) : [focused!];
      const off = targets.length > 0 && targets.every((p) => p.is_favourite);
      const action: BulkAction = { action: off ? "unfavourite" : "favourite" };
      if (many) bulkSelection.mutate({ ...action, ids: selection.ids });
      else bulkOne.mutate({ ...action, ids: [focused!.id] });
      return;
    }
    // A single card goes straight to the trash when there is one (the toast has Undo).
    if (kind === "delete" && !many && trashDays > 0) return bulkOne.mutate({ action: "delete", ids: [focused!.id] });
    setDialog(many ? { kind, scope: "selection" } : { kind, scope: "project", id: focused!.id });
  };

  const keys: ShortcutMap = {
    ArrowLeft: move("left"),
    ArrowRight: move("right"),
    ArrowUp: move("up"),
    ArrowDown: move("down"),
    h: move("left"),
    l: move("right"),
    k: move("up"),
    j: move("down"),
    Enter: () => (focused ? open(focused.id) : false),
    o: () => (focused ? open(focused.id) : false),
    x: () => {
      if (!focused) return false;
      if (!selection.active) selection.start();
      selection.toggle(focused.id, false);
    },
    X: () => {
      if (items.length === 0) return false;
      if (!selection.active) selection.start();
      (onSelectAll ?? selection.selectAll)();
    },
    f: act("favourite"),
    d: act("download"),
    t: act("tags"),
    a: act("collections"),
    "#": act("delete"),
    Delete: act("delete"),
    Escape: () => {
      if (selection.active) return selection.exit();
      if (index !== null) return setFocus(null);
      return false;
    },
  };
  useShortcuts(keys);

  /** Runs a bulk action from one of the dialogs on what the dialog was opened for. */
  const run = (action: BulkAction) => {
    if (!dialog) return;
    if (dialog.scope === "selection") bulkSelection.mutate({ ...action, ids: selection.ids } as Parameters<typeof bulkSelection.mutate>[0]);
    else bulkOne.mutate({ ...action, ids: [dialog.id] } as Parameters<typeof bulkOne.mutate>[0]);
    setDialog(null);
  };

  // Live from the list, so the pickers show changes as they happen.
  const dialogProject = dialog?.scope === "project" ? (items.find((p) => p.id === dialog.id) ?? null) : null;

  return {
    focusedId: focused?.id ?? null,
    remember,
    dialog,
    dialogProject,
    closeDialog: () => setDialog(null),
    run,
    selectionCount: selection.ids.length,
  };
}

export type GridKeys = ReturnType<typeof useGridKeys>;
