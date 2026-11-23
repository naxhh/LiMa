import * as React from "react";
import { Link, NavLink, useLocation, useSearchParams } from "react-router-dom";
import { Layers, LayoutGrid, RefreshCw, Search, Tag as TagIcon } from "lucide-react";

import { cn } from "@/lib/utils";
import { ThemeToggle } from "@/components/theme-toggle";
import { CreateProjectDialog } from "@/components/create-project-dialog";
import { ImportUrlDialog } from "@/components/import-url-dialog";
import { SearchPalette } from "@/components/search-palette";

type NavItem = { to: string; label: string; icon: React.ComponentType<{ className?: string }>; end?: boolean };

const NAV: NavItem[] = [
  { to: "/", label: "Library", icon: LayoutGrid, end: true },
  { to: "/tags", label: "Tags", icon: TagIcon },
  { to: "/collections", label: "Collections", icon: Layers },
  { to: "/admin/sync", label: "Sync", icon: RefreshCw },
];

/** The LiMa mark: a lime slice drawn as a circuit trace. Uses the theme's accent colors. */
export function LogoMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 512 512" aria-hidden="true" className={cn("text-primary-foreground", className)}>
      <rect width="512" height="512" rx="112" fill="var(--primary)" />
      <g transform="translate(-12 14)">
        <path d="M267.7 88.4 A168 168 0 1 0 334.9 107.7 L380.9 61.7 L414.9 61.7" fill="none" stroke="currentColor" strokeWidth="26" strokeLinecap="round" strokeLinejoin="round"/>
        <circle cx="432.9" cy="61.7" r="20" fill="none" stroke="currentColor" strokeWidth="16"/>
        <path d="M271.0 218.9 L271.0 132.9 A124 124 0 0 1 355.1 181.5 L280.6 224.5 A40 40 0 0 0 271.0 218.9 Z" fill="currentColor" stroke="currentColor" strokeWidth="6" strokeLinejoin="round"/>
        <path d="M295.6 250.4 L370.1 207.4 A124 124 0 0 1 370.1 304.6 L295.6 261.6 A40 40 0 0 0 295.6 250.4 Z" fill="currentColor" stroke="currentColor" strokeWidth="6" strokeLinejoin="round"/>
        <path d="M280.6 287.5 L355.1 330.5 A124 124 0 0 1 271.0 379.1 L271.0 293.1 A40 40 0 0 0 280.6 287.5 Z" fill="currentColor" stroke="currentColor" strokeWidth="6" strokeLinejoin="round"/>
        <path d="M241.0 293.1 L241.0 379.1 A124 124 0 0 1 156.9 330.5 L231.4 287.5 A40 40 0 0 0 241.0 293.1 Z" fill="currentColor" stroke="currentColor" strokeWidth="6" strokeLinejoin="round"/>
        <path d="M216.4 261.6 L141.9 304.6 A124 124 0 0 1 141.9 207.4 L216.4 250.4 A40 40 0 0 0 216.4 261.6 Z" fill="currentColor" stroke="currentColor" strokeWidth="6" strokeLinejoin="round"/>
        <path d="M231.4 224.5 L156.9 181.5 A124 124 0 0 1 241.0 132.9 L241.0 218.9 A40 40 0 0 0 231.4 224.5 Z" fill="currentColor" stroke="currentColor" strokeWidth="6" strokeLinejoin="round"/>
      </g>
    </svg>
  );
}

export function Logo() {
  return (
    <Link to="/" className="flex items-center gap-2.5 text-foreground">
      <LogoMark className="size-[30px]" />
      <span className="font-display text-xl font-bold tracking-tight">LiMa</span>
    </Link>
  );
}

/** The header search: shows the library query and opens the search palette (also ⌘K / Ctrl+K, or /). */
function HeaderSearch() {
  const location = useLocation();
  const [params] = useSearchParams();
  const query = location.pathname === "/" ? (params.get("query") ?? "") : "";
  const [open, setOpen] = React.useState(false);

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && (e.target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName));
      if (((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") || (e.key === "/" && !typing)) {
        e.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <button
        type="button"
        role="search"
        aria-label="Search projects"
        onClick={() => setOpen(true)}
        className="flex h-10 min-w-0 flex-1 items-center gap-2.5 rounded-lg border border-input bg-card px-3 text-left text-sm hover:border-line-strong focus-visible:border-ring focus-visible:outline-none"
      >
        <Search className="size-4 shrink-0 text-dim" aria-hidden />
        <span className={cn("min-w-0 flex-1 truncate", query ? "text-foreground" : "text-dim")}>{query || "Search projects, #tags, in:collections…"}</span>
        <kbd className="hidden rounded border border-input px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground sm:inline">⌘K</kbd>
      </button>
      <SearchPalette open={open} onOpenChange={setOpen} initial={query} />
    </>
  );
}

export function AppHeader() {
  return (
    <header className="sticky top-0 z-30 border-b bg-background/85 backdrop-blur-md">
      <div className="mx-auto flex max-w-[1440px] flex-wrap items-center gap-x-6 gap-y-3 px-4 py-3 sm:px-8">
        <Logo />
        <nav aria-label="Main" className="hidden gap-1 md:flex">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                cn(
                  "rounded-lg px-3 py-2 text-sm transition-colors",
                  isActive ? "bg-secondary font-medium text-foreground" : "text-muted-foreground hover:text-foreground",
                )
              }
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
        <div className="order-last flex w-full md:order-none md:w-auto md:flex-1">
          <HeaderSearch />
        </div>
        <div className="ml-auto flex items-center gap-2 md:ml-0">
          <ImportUrlDialog />
          <CreateProjectDialog />
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}

/** Bottom tab bar on small screens. */
export function MobileNav() {
  return (
    <nav
      aria-label="Main"
      className="fixed inset-x-0 bottom-0 z-30 grid grid-cols-4 border-t bg-card/95 px-2 pt-1.5 pb-[max(0.5rem,env(safe-area-inset-bottom))] backdrop-blur md:hidden"
    >
      {NAV.map((item) => {
        const Icon = item.icon;
        return (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) =>
              cn(
                "flex h-13 flex-col items-center justify-center gap-1 text-[11px]",
                isActive ? "font-medium text-signal" : "text-muted-foreground",
              )
            }
          >
            <Icon className="size-5" />
            {item.label}
          </NavLink>
        );
      })}
    </nav>
  );
}

/** Page title block on the hairline grid: `// 01 — LIBRARY` label, big title, optional right side. */
export function PageHero({
  index,
  label,
  title,
  subtitle,
  children,
}: {
  index: string;
  label: string;
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <section className="bg-grid border-b">
      <div className="mx-auto flex max-w-[1440px] flex-wrap items-end justify-between gap-6 px-4 pt-10 pb-8 sm:px-8 sm:pt-12">
        <div className="flex min-w-0 flex-col gap-2.5">
          <span className="font-mono text-xs tracking-[0.12em] text-signal">
            // {index} — {label.toUpperCase()}
          </span>
          <h1 className="font-display text-4xl leading-none font-semibold tracking-[-0.03em] sm:text-[52px]">{title}</h1>
          {subtitle ? <div className="font-mono text-[13px] text-muted-foreground">{subtitle}</div> : null}
        </div>
        {children}
      </div>
    </section>
  );
}

/** Centered page body with the standard gutters. */
export function PageBody({ className, children }: { className?: string; children: React.ReactNode }) {
  return <div className={cn("mx-auto max-w-[1440px] px-4 sm:px-8", className)}>{children}</div>;
}

/** Small uppercase mono label. */
export function MicroLabel({ className, children }: { className?: string; children: React.ReactNode }) {
  return <span className={cn("font-mono text-[10px] tracking-[0.1em] text-dim uppercase", className)}>{children}</span>;
}
