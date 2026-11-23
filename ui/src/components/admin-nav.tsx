import type { ComponentType } from "react";
import { NavLink } from "react-router-dom";
import { Copy, RefreshCw, Trash2 } from "lucide-react";

import { cn } from "@/lib/utils";

export type AdminSection = { to: string; label: string; icon: ComponentType<{ className?: string }>; index: string };

/** The admin sub-pages, in order, with their `// 04v…` section numbers. */
export const ADMIN_SECTIONS: AdminSection[] = [
  { to: "/admin/sync", label: "Sync", icon: RefreshCw, index: "04v1" },
  { to: "/admin/trash", label: "Trash", icon: Trash2, index: "04v2" },
  { to: "/admin/duplicates", label: "Duplicates", icon: Copy, index: "04v3" },
];

export function adminIndex(to: string): string {
  return ADMIN_SECTIONS.find((s) => s.to === to)?.index ?? "04";
}

/** The shared admin sub-menu: a tab strip rendered under the PageHero on every admin page. */
export function AdminNav({ trashCount, duplicatesCount }: { trashCount?: number; duplicatesCount?: number }) {
  const counts: Record<string, number | undefined> = {
    "/admin/trash": trashCount,
    "/admin/duplicates": duplicatesCount,
  };
  return (
    <nav aria-label="Admin" className="flex gap-1 overflow-x-auto border-b">
      {ADMIN_SECTIONS.map((section) => {
        const Icon = section.icon;
        return (
          <NavLink
            key={section.to}
            to={section.to}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-2 border-b-2 px-3.5 py-2.5 text-sm whitespace-nowrap -mb-px transition-colors",
                isActive ? "border-signal font-medium text-foreground" : "border-transparent text-muted-foreground hover:text-foreground",
              )
            }
          >
            <Icon className="size-4" />
            {section.label}
            <span className="font-mono text-[10px] text-dim">{section.index}</span>
            {counts[section.to] != null ? (
              <span className="rounded-full border border-input px-1.5 py-px font-mono text-[10px] leading-4 text-muted-foreground">
                {counts[section.to]}
              </span>
            ) : null}
          </NavLink>
        );
      })}
    </nav>
  );
}
