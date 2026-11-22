import * as React from "react";
import { Link, NavLink, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { Box, Layers, LayoutGrid, RefreshCw, Search, Tag as TagIcon } from "lucide-react";

import { cn } from "@/lib/utils";
import { ThemeToggle } from "@/components/theme-toggle";
import { CreateProjectDialog } from "@/components/create-project-dialog";

type NavItem = { to: string; label: string; icon: React.ComponentType<{ className?: string }>; end?: boolean };

const NAV: NavItem[] = [
  { to: "/", label: "Library", icon: LayoutGrid, end: true },
  { to: "/tags", label: "Tags", icon: TagIcon },
  { to: "/collections", label: "Collections", icon: Layers },
  { to: "/admin/sync", label: "Sync", icon: RefreshCw },
];

export function Logo() {
  return (
    <Link to="/" className="flex items-center gap-2.5 text-foreground">
      <span className="grid size-[30px] place-items-center rounded-lg bg-primary text-primary-foreground">
        <Box className="size-[18px]" strokeWidth={2.2} />
      </span>
      <span className="font-display text-xl font-bold tracking-tight">LiMa</span>
    </Link>
  );
}

/** Search box bound to `?query=` on the library page; elsewhere Enter jumps to the library. */
function HeaderSearch() {
  const location = useLocation();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const onLibrary = location.pathname === "/";
  const urlQuery = onLibrary ? params.get("query") ?? "" : "";
  const [value, setValue] = React.useState(urlQuery);
  const inputRef = React.useRef<HTMLInputElement>(null);

  // Follow navigation (e.g. a tag link) but not our own keystrokes.
  React.useEffect(() => {
    setValue((cur) => (cur.trim() === urlQuery ? cur : urlQuery));
  }, [urlQuery]);

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const update = (next: string) => {
    setValue(next);
    if (onLibrary) {
      const trimmed = next.trim();
      setParams(trimmed ? { query: trimmed } : {}, { replace: true });
    }
  };

  return (
    <form
      role="search"
      className="flex h-10 min-w-0 flex-1 items-center gap-2.5 rounded-lg border border-input bg-card px-3 focus-within:border-ring"
      onSubmit={(e) => {
        e.preventDefault();
        const trimmed = value.trim();
        navigate(trimmed ? `/?query=${encodeURIComponent(trimmed)}` : "/");
      }}
    >
      <Search className="size-4 shrink-0 text-dim" aria-hidden />
      <label htmlFor="global-search" className="sr-only">
        Search projects
      </label>
      <input
        id="global-search"
        ref={inputRef}
        type="search"
        value={value}
        onChange={(e) => update(e.target.value)}
        placeholder="Search projects, files, tags…"
        autoComplete="off"
        className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-dim"
      />
      <kbd className="hidden rounded border border-input px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground sm:inline">
        ⌘K
      </kbd>
    </form>
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
