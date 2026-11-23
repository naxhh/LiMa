import * as React from "react";
import { useNavigate } from "react-router-dom";

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  ShortcutsContext,
  isTypingTarget,
  overlayOpen,
  type ShortcutEntry,
  type ShortcutsContextValue,
} from "@/lib/shortcuts";
import { cn } from "@/lib/utils";

/** `g` then one of these keys goes to a section. */
const GO_TO: Record<string, { to: string; label: string }> = {
  l: { to: "/", label: "library" },
  t: { to: "/tags", label: "tags" },
  c: { to: "/collections", label: "collections" },
  a: { to: "/admin/sync", label: "admin" },
  s: { to: "/admin/sync", label: "sync" },
  r: { to: "/admin/trash", label: "trash" },
  d: { to: "/admin/duplicates", label: "duplicates" },
  x: { to: "/admin/tags", label: "admin tags" },
};

const CHORD_MS = 1200;

/** Keys that may auto-repeat while held (moving around); everything else fires once per press. */
const REPEATABLE = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "h", "j", "k", "l"]);

/** Elements that use these keys themselves when focused. */
const ACTIVATES = "a[href], button, summary, [role=button], [role=link], [role=checkbox], [role=switch]";
const USES_ARROWS = "[role=slider], [role=tab], [role=radio], [role=option], [role=menuitem], canvas";

function keyName(e: KeyboardEvent): string {
  if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "k") return "mod+k";
  return e.key;
}

/** Owns the window key listener, the `g` chord HUD and the `?` cheat sheet. Mount once, inside the router. */
export function ShortcutsProvider({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const entries = React.useRef<ShortcutEntry[]>([]);
  const [help, setHelp] = React.useState(false);
  // null: no chord; "": waiting for the key after `g`; else the section we're going to.
  const [chord, setChord] = React.useState<string | null>(null);
  const chordRef = React.useRef(false);
  const chordTimer = React.useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // useNavigate changes on every navigation; the listener (and its timers) must outlive that.
  const navigateRef = React.useRef(navigate);
  React.useEffect(() => {
    navigateRef.current = navigate;
  });

  const value = React.useMemo<ShortcutsContextValue>(
    () => ({
      register: (entry) => {
        entries.current = [...entries.current, entry];
        return () => {
          entries.current = entries.current.filter((x) => x !== entry);
        };
      },
      openHelp: () => setHelp(true),
    }),
    [],
  );

  React.useEffect(() => {
    const endChord = (label: string | null) => {
      chordRef.current = false;
      clearTimeout(chordTimer.current);
      if (label === null) return setChord(null);
      setChord(label);
      chordTimer.current = setTimeout(() => setChord(null), 600);
    };

    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const key = keyName(e);
      const handle = (fn: () => boolean | void) => {
        if (fn() === false) return false;
        e.preventDefault();
        return true;
      };
      const dispatch = () => {
        // Pages first (latest mounted first), then the global ones.
        const ordered = [...entries.current].reverse().sort((a, b) => Number(a.global) - Number(b.global));
        for (const entry of ordered) {
          const fn = entry.map.current?.[key];
          if (fn && handle(() => fn(e))) return;
        }
      };

      // ⌘K / Ctrl+K works from anywhere, even while typing.
      if (key === "mod+k") return dispatch();
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (isTypingTarget(e.target) || overlayOpen()) return;

      if (chordRef.current) {
        e.preventDefault();
        const dest = GO_TO[e.key.toLowerCase()];
        if (!dest) return endChord(null);
        endChord(dest.label);
        void navigateRef.current(dest.to);
        return;
      }

      if (e.repeat && !REPEATABLE.has(key)) return;
      const target = e.target instanceof Element && e.target !== document.body ? e.target : null;
      if (target && (key === "Enter" || key === " ") && target.closest(ACTIVATES)) return;
      if (target && key.startsWith("Arrow") && target.closest(USES_ARROWS)) return;

      if (key === "g") {
        e.preventDefault();
        chordRef.current = true;
        setChord("");
        clearTimeout(chordTimer.current);
        chordTimer.current = setTimeout(() => endChord(null), CHORD_MS);
        return;
      }
      if (key === "?") {
        e.preventDefault();
        setHelp(true);
        return;
      }
      dispatch();
    };

    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      clearTimeout(chordTimer.current);
    };
  }, []);

  return (
    <ShortcutsContext.Provider value={value}>
      {children}
      {chord !== null ? (
        <div
          role="status"
          className="fixed bottom-[88px] left-4 z-50 rounded-md border bg-card/90 px-2.5 py-1.5 font-mono text-xs shadow backdrop-blur md:bottom-4"
        >
          {chord ? `g → ${chord}` : "g…"}
        </div>
      ) : null}
      <ShortcutsHelp open={help} onOpenChange={setHelp} />
    </ShortcutsContext.Provider>
  );
}

type Row = { keys: string[][]; label: string };

const SECTIONS: { title: string; rows: Row[] }[] = [
  {
    title: "Everywhere",
    rows: [
      { keys: [["/"], ["⌘", "K"]], label: "Search" },
      { keys: [["c"]], label: "Create menu" },
      { keys: [["g", "l"]], label: "Go to Library" },
      { keys: [["g", "t"]], label: "Go to Tags" },
      { keys: [["g", "c"]], label: "Go to Collections" },
      { keys: [["g", "a"], ["g", "s"]], label: "Go to Admin · Sync" },
      { keys: [["g", "r"]], label: "Go to Trash" },
      { keys: [["g", "d"]], label: "Go to Duplicates" },
      { keys: [["g", "x"]], label: "Go to Admin · Tags" },
      { keys: [["?"]], label: "This cheat sheet" },
      { keys: [["Esc"]], label: "Close the dialog, or clear the selection" },
    ],
  },
  {
    title: "Lists",
    rows: [
      { keys: [["←", "↑", "↓", "→"], ["h", "j", "k", "l"]], label: "Move between cards" },
      { keys: [["Enter"], ["o"]], label: "Open the project" },
      { keys: [["x"]], label: "Select the card" },
      { keys: [["⇧", "X"]], label: "Select every match" },
      { keys: [["f"]], label: "Favourite" },
      { keys: [["d"]], label: "Download" },
      { keys: [["t"]], label: "Tags" },
      { keys: [["a"]], label: "Add to collection" },
      { keys: [["#"], ["Del"]], label: "Move to the trash" },
    ],
  },
  {
    title: "Project page",
    rows: [
      { keys: [["u"]], label: "Back to the list" },
      { keys: [["j"], ["k"]], label: "Next / previous project" },
      { keys: [["←"], ["→"]], label: "Previous / next media" },
      { keys: [["f"], ["d"], ["t"], ["a"], ["#"]], label: "Favourite, download, tags, collection, remove" },
      { keys: [["e"]], label: "Edit details" },
      { keys: [["i"]], label: "Add files" },
    ],
  },
  {
    title: "Create menu",
    rows: [
      { keys: [["p"]], label: "New project" },
      { keys: [["i"]], label: "Import from URL" },
      { keys: [["c"]], label: "New collection" },
      { keys: [["t"]], label: "New tag" },
    ],
  },
];

export function Kbd({ className, children }: { className?: string; children: React.ReactNode }) {
  return <kbd className={cn("rounded border border-input px-1.5 py-0.5 font-mono text-[11px]", className)}>{children}</kbd>;
}

function ShortcutsHelp({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Keys do nothing while you type in a field or a dialog is open (except Esc).</DialogDescription>
        </DialogHeader>
        <div className="grid gap-x-8 gap-y-6 sm:grid-cols-2">
          {SECTIONS.map((s) => (
            <section key={s.title} className="flex flex-col gap-2">
              <h3 className="font-mono text-[10px] tracking-[0.1em] text-dim uppercase">{s.title}</h3>
              <dl className="flex flex-col gap-1.5 text-sm">
                {s.rows.map((r) => (
                  <div key={r.label} className="flex items-center justify-between gap-3">
                    <dt className="text-muted-foreground">{r.label}</dt>
                    <dd className="flex shrink-0 items-center gap-1">
                      {r.keys.map((combo, i) => (
                        <React.Fragment key={i}>
                          {i > 0 ? <span className="px-0.5 text-[11px] text-dim">or</span> : null}
                          {combo.map((k) => (
                            <Kbd key={k}>{k}</Kbd>
                          ))}
                        </React.Fragment>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
