import * as React from "react";

/**
 * Multi-select over an ordered list of ids: click toggles, shift-click selects the range from the
 * last clicked card, Escape leaves selection mode.
 */
export function useSelection(ids: string[]) {
  const [active, setActive] = React.useState(false);
  const [selected, setSelected] = React.useState<Set<string>>(() => new Set());
  const anchor = React.useRef<string | null>(null);

  const exit = React.useCallback(() => {
    setActive(false);
    setSelected(new Set());
    anchor.current = null;
  }, []);

  React.useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector("[role=dialog]")) exit();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, exit]);

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

  // Only ids still on screen count (deleted or filtered-out cards drop out).
  const chosen = ids.filter((id) => selected.has(id));

  return {
    active,
    start: () => setActive(true),
    exit,
    toggle,
    isSelected: (id: string) => selected.has(id),
    selectAll: () => setSelected(new Set(ids)),
    clear: () => setSelected(new Set()),
    ids: chosen,
  };
}

export type Selection = ReturnType<typeof useSelection>;
