import * as React from "react";

/**
 * Keyboard shortcuts. One window listener (ShortcutsProvider) dispatches keys to the handlers
 * registered with `useShortcuts`; page registrations win over global ones (header, menus).
 *
 * Keys are `KeyboardEvent.key` values: "f", "X" (Shift+x), "?", "#", "Enter", "Escape",
 * "ArrowLeft", "Delete"…, plus "mod+k" for ⌘K / Ctrl+K. A handler returns `false` when it
 * didn't use the key, so the next registration (or the browser) gets it.
 */
export type ShortcutHandler = (e: KeyboardEvent) => boolean | void;
export type ShortcutMap = Partial<Record<string, ShortcutHandler>>;

export type ShortcutEntry = { map: React.RefObject<ShortcutMap | null>; global: boolean };

export type ShortcutsContextValue = {
  register: (entry: ShortcutEntry) => () => void;
  openHelp: () => void;
};

export const ShortcutsContext = React.createContext<ShortcutsContextValue | null>(null);

export function useShortcutsContext(): ShortcutsContextValue {
  const ctx = React.useContext(ShortcutsContext);
  if (!ctx) throw new Error("useShortcutsContext must be used within a ShortcutsProvider");
  return ctx;
}

/**
 * Registers keys while the component is mounted. The latest `map` is always used, so handlers
 * can close over fresh state. `global` registrations only get keys no page claimed.
 */
export function useShortcuts(map: ShortcutMap | null, { global = false }: { global?: boolean } = {}) {
  const { register } = useShortcutsContext();
  const ref = React.useRef<ShortcutMap | null>(map);
  React.useEffect(() => {
    ref.current = map;
  });
  React.useEffect(() => register({ map: ref, global }), [register, global]);
}

/** True while the user types somewhere: keys belong to that field. */
export function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
}

/** A Radix dialog, popover, menu or listbox is open: plain keys belong to it. */
export function overlayOpen(): boolean {
  return !!document.querySelector(
    "[role=dialog][data-state=open], [role=alertdialog][data-state=open], [role=menu], [role=listbox]",
  );
}

/** Starts a download of a URL the way an `<a download>` click would. */
export function downloadUrl(url: string) {
  const a = document.createElement("a");
  a.href = url;
  a.download = "";
  document.body.appendChild(a);
  a.click();
  a.remove();
}
