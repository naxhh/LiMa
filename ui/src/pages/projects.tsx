import * as React from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiNoBody } from "@/lib/api";

import { apiGet } from "@/lib/api";
import { Input } from "@/components/ui/input";
import { CreateProjectDialog } from "@/components/create-project-dialog";
import { MediaCard } from "@/components/media-card";

import { Link } from "react-router-dom";
import { MoreHorizontal } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";



type ProjectRow = {
  id: string;
  folder_path: string;
  name: string;
  description: string;
  main_image_id: string | null;
  created_at: string;
  updated_at: string;
  last_scanned_at?: string | null;
};

type ListProjectsResponse = {
  items: ProjectRow[];
  next_cursor: string | null;
};

function buildUrl(params: { limit: number; cursor?: string | null; query?: string }) {
  const usp = new URLSearchParams();
  usp.set("limit", String(params.limit));
  if (params.cursor) usp.set("cursor", params.cursor);
  if (params.query && params.query.trim()) usp.set("query", params.query.trim());
  return `/projects?${usp.toString()}`;
}

function useDebouncedValue<T>(value: T, delayMs: number) {
  const [debounced, setDebounced] = React.useState(value);
  React.useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(t);
  }, [value, delayMs]);
  return debounced;
}

export function ProjectsPage() {
  const deleteProjectM = useMutation({
    mutationFn: (projectId: string) => apiNoBody("DELETE", `/projects/${projectId}`),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  const [q, setQ] = React.useState("");
  const debouncedQ = useDebouncedValue(q, 250);
  const limit = 50;

  const projectsQ = useInfiniteQuery({
    queryKey: ["projects", { limit, query: debouncedQ }],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      apiGet<ListProjectsResponse>(
        buildUrl({ limit, cursor: pageParam, query: debouncedQ })
      ),
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
  });

  const items =
    projectsQ.data?.pages.flatMap((p) => p.items) ?? [];

  const sentinelRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    const el = sentinelRef.current;
    if (!el) return;

    const io = new IntersectionObserver((entries) => {
      const first = entries[0];
      if (first?.isIntersecting && projectsQ.hasNextPage && !projectsQ.isFetchingNextPage) {
        projectsQ.fetchNextPage();
      }
    });

    io.observe(el);
    return () => io.disconnect();
  }, [projectsQ.hasNextPage, projectsQ.isFetchingNextPage, projectsQ.fetchNextPage]);


  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold">Projects</h1>
        <CreateProjectDialog />
      </div>


      <div className="flex items-center gap-2">
        <Input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search projects…"
        />
        {q ? (
          <Button variant="outline" onClick={() => setQ("")}>
            Clear
          </Button>
        ) : null}
      </div>

      {projectsQ.isLoading ? <div>Loading…</div> : null}

      {projectsQ.isError ? (
        <div className="text-sm text-destructive">
          Failed to load projects.
        </div>
      ) : null}

      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
        {items.map((p) => {
          const tags = (p as any).tags ?? [];

          return (
            <MediaCard
              href={`/projects/${p.id}`}
              title={p.name}
              subtitle={p.description?.trim() ? p.description : "—"}
              meta={`Updated: ${p.updated_at}`}
              chips={[
                 ...tags.map((t: string) => ({ label: t })),  
              ]}
              placeholder={p.main_image_id ? "main image preview" : "no preview"}
              actions={[
                {
                  label: "Delete",
                  destructive: true,
                  onClick: () => deleteProjectM.mutate(p.id),
                  disabled: deleteProjectM.isPending,
                },
              ]}
            />
          );
        })}
      </div>



      <div ref={sentinelRef} className="h-8" />
      {projectsQ.isFetchingNextPage ? <div>Loading more…</div> : null}
    </div>
  );
}
