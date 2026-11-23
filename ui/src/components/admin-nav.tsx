import * as React from "react";
import { NavLink } from "react-router-dom";

import { ADMIN_SECTIONS } from "@/lib/admin";
import { cn } from "@/lib/utils";

/** The shared admin sub-menu: a tab strip rendered under the PageHero on every admin page. */
export function AdminNav({
  trashCount,
  duplicatesCount,
  actions,
}: {
  trashCount?: number;
  duplicatesCount?: number;
  actions?: React.ReactNode;
}) {
  const counts: Record<string, number | undefined> = {
    "/admin/trash": trashCount,
    "/admin/duplicates": duplicatesCount,
  };
  return (
    <nav aria-label="Admin" className="flex flex-wrap items-end border-b">
      <div className="-mb-px flex min-w-0 flex-1 basis-full gap-1 overflow-x-auto overflow-y-hidden [scrollbar-width:none] [&::-webkit-scrollbar]:hidden sm:basis-auto">
        {ADMIN_SECTIONS.map((section) => {
          const Icon = section.icon;
          return (
            <NavLink
              key={section.to}
              to={section.to}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-2 border-b-2 px-3.5 py-2.5 text-sm whitespace-nowrap transition-colors",
                  isActive ? "border-signal font-medium text-foreground" : "border-transparent text-muted-foreground hover:text-foreground",
                )
              }
            >
              <Icon className="size-4" />
              {section.label}
              <span className="hidden font-mono text-[10px] text-dim sm:inline">{section.index}</span>
              {counts[section.to] != null ? (
                <span className="rounded-full border border-input px-1.5 py-px font-mono text-[10px] leading-4 text-muted-foreground">
                  {counts[section.to]}
                </span>
              ) : null}
            </NavLink>
          );
        })}
      </div>
      {actions ? <div className="ml-auto flex shrink-0 items-center gap-2 pl-4 pb-1.5">{actions}</div> : null}
    </nav>
  );
}
