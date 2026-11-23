import { NavLink } from "react-router-dom";

import { ADMIN_SECTIONS } from "@/lib/admin";
import { cn } from "@/lib/utils";

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
