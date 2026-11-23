import * as React from "react";

/**
 * Multi-select over an ordered list of ids: click toggles, shift-click selects the range from the
 * last clicked card. Escape (see useGridKeys) leaves selection mode.
 */
export function useSelection(ids: string[]) {
  const [active, setActive] = React.useState(false);
  const [selected, setSelected] = React.useState<Set<string>>(() => new Set());
  // Set by selectIds: everything matching, including cards not loaded yet.
  const [beyond, setBeyond] = React.useState(false);
  const anchor = React.useRef<string | null>(null);

  const exit = React.useCallback(() => {
    setActive(false);
    setSelected(new Set());
    setBeyond(false);
    anchor.current = null;
  }, []);

  const toggle = (id: string, range: boolean) => {
    // Read the anchor now: the state updater runs later, after it has moved.
    const from = anchor.current ? ids.indexOf(anchor.current) : -1;
    const to = ids.indexOf(id);
    setSelected((prev) => {
      const next = new Set(prev);
      if (range && from >= 0 && to >= 0) {
        const [a, b] = from < to ? [from, to] : [to, from];
        for (const i of ids.slice(a, b + 1)) next.add(i);
      } else if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
    anchor.current = id;
  };

  // Only ids still on screen count (deleted or filtered-out cards drop out), unless every
  // match was selected, cards not loaded yet included.
  const chosen = beyond ? [...selected] : ids.filter((id) => selected.has(id));

  return {
    active,
    start: () => setActive(true),
    exit,
    toggle,
    isSelected: (id: string) => selected.has(id),
    selectAll: () => {
      setBeyond(false);
      setSelected(new Set(ids));
    },
    /** Selects these ids, loaded or not (from the server's list of every match). */
    selectIds: (all: string[]) => {
      setBeyond(true);
      setSelected(new Set(all));
    },
    clear: () => {
      setBeyond(false);
      setSelected(new Set());
    },
    ids: chosen,
  };
}

export type Selection = ReturnType<typeof useSelection>;
