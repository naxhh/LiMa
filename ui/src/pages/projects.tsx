import { useQuery } from "@tanstack/react-query";
import { apiGet } from "@/lib/api";
import { Link } from "react-router-dom";

type Project = {
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
  items: Project[];
  next_cursor: string | null;
};

export function ProjectsPage() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["projects", { limit: 50 }],
    queryFn: () => apiGet<ListProjectsResponse>(`/projects?limit=50`),
  });

  if (isLoading) return <div className="p-6">Loading…</div>;
  if (error) return <div className="p-6">Error loading projects</div>;

  return (
    <div className="p-6 space-y-4">
      <h1 className="text-2xl font-semibold">Projects</h1>

      <div className="space-y-2">
        {data?.items.map((p: Project) => (
          <div key={p.id} className="rounded-lg border p-3">
            <Link to={`/projects/${p.id}`} className="block rounded-lg border p-3 hover:bg-muted/40">
              <div className="font-medium">{p.name}</div>
              <div className="text-sm text-muted-foreground">{p.description}</div>
            </Link>
          </div>
        ))}
      </div>
    </div>
  );
}
