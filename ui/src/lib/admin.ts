import type { ComponentType } from "react";
import { Copy, RefreshCw, Tags, Trash2 } from "lucide-react";

export type AdminSection = { to: string; label: string; icon: ComponentType<{ className?: string }>; index: string };

/** The admin sub-pages, in order, with their `// 04v…` section numbers. */
export const ADMIN_SECTIONS: AdminSection[] = [
  { to: "/admin/sync", label: "Sync", icon: RefreshCw, index: "04v1" },
  { to: "/admin/trash", label: "Trash", icon: Trash2, index: "04v2" },
  { to: "/admin/duplicates", label: "Duplicates", icon: Copy, index: "04v3" },
  { to: "/admin/tags", label: "Tags", icon: Tags, index: "04v4" },
];

export function adminIndex(to: string): string {
  return ADMIN_SECTIONS.find((s) => s.to === to)?.index ?? "04";
}
