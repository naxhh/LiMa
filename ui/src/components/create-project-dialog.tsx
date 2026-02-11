import * as React from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson, ApiError } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

type CreateProjectRequest = {
  name: string;
  description?: string | null;
  tags?: string[] | null;
};

type CreateProjectResponse = {
  id: string;
  folder_path: string;
};

function getApiErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const body = e.body as any;
    const msg = body?.error?.message || body?.message;
    return msg ? `${msg}` : `Request failed (${e.status})`;
  }
  return "Request failed";
}

function parseTags(input: string): string[] {
  // comma or newline separated, trimmed, unique, non-empty
  const raw = input
    .split(/[,|\n]/g)
    .map((s) => s.trim())
    .filter(Boolean);

  const seen = new Set<string>();
  const out: string[] = [];
  for (const t of raw) {
    const key = t.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(t);
  }
  return out;
}

export function CreateProjectDialog() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const [open, setOpen] = React.useState(false);

  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [tagsText, setTagsText] = React.useState("");

  const createM = useMutation({
    mutationFn: (payload: CreateProjectRequest) =>
      apiJson<CreateProjectResponse>("POST", "/projects", payload),
    onSuccess: async (res) => {
      // refresh list
      await qc.invalidateQueries({ queryKey: ["projects"] });
      setOpen(false);

      // reset fields
      setName("");
      setDescription("");
      setTagsText("");

      // go to project
      navigate(`/projects/${res.id}`);
    },
  });

  const canSubmit = name.trim().length > 0;

  return (
    <Dialog open={open} onOpenChange={(v) => {
      setOpen(v);
      if (!v) createM.reset();
    }}>
      <DialogTrigger asChild>
        <Button>Create project</Button>
      </DialogTrigger>

      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create project</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1">
            <div className="text-sm font-medium">Name</div>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Dice tray v2"
              autoFocus
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Description</div>
            <Textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional"
            />
          </div>

          <div className="space-y-1">
            <div className="text-sm font-medium">Tags</div>
            <Textarea
              value={tagsText}
              onChange={(e) => setTagsText(e.target.value)}
              placeholder="Comma or newline separated (e.g. tabletop, printing)"
            />
            <div className="text-xs text-muted-foreground">
              Creates missing tags automatically.
            </div>
          </div>

          {createM.isError ? (
            <div className="text-sm text-destructive">
              {getApiErrorMessage(createM.error)}
            </div>
          ) : null}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => setOpen(false)}
            disabled={createM.isPending}
          >
            Cancel
          </Button>

          <Button
            disabled={!canSubmit || createM.isPending}
            onClick={() => {
              const tags = parseTags(tagsText);
              createM.mutate({
                name: name.trim(),
                description: description.trim() ? description : "",
                tags: tags.length ? tags : [],
              });
            }}
          >
            {createM.isPending ? "Creating…" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
