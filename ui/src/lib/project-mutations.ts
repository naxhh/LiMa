import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson, apiNoBody } from "@/lib/api";
import { COLLECTIONS_QUERY_KEY, type Collection } from "@/lib/collections";
import { TAGS_QUERY_KEY, type Tag } from "@/lib/tags";
import { PROJECT_CACHE_KEYS, optimisticHandlers, patchProjects } from "@/lib/optimistic";
import { TRASH_KEY, restoreFromTrash } from "@/lib/trash";
import { toast } from "sonner";

export type ProjectTag = { id: string; name: string; color: string };
export type Membership = { id: string; name: string };

/** Every change to one project shares this key, so they queue in order. */
export const projectMutationKey = (projectId: string) => ["project-edit", projectId] as const;

const PROJECT_AND_LISTS = [...PROJECT_CACHE_KEYS, TAGS_QUERY_KEY, COLLECTIONS_QUERY_KEY];

/** Saves a project's full tag list. Shows the new tags immediately. */
export function useSaveProjectTags(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (names: string[]) => apiJson<void>("PATCH", `/projects/${projectId}`, { tags: names }),
    ...optimisticHandlers<string[]>(qc, {
      mutationKey: projectMutationKey(projectId),
      keys: PROJECT_AND_LISTS,
      apply: (names) => {
        const known = new Map((qc.getQueryData<Tag[]>(TAGS_QUERY_KEY) ?? []).map((t) => [t.name.toLowerCase(), t]));
        const tags = names.map((name) => {
          const t = known.get(name.toLowerCase());
          // New tags get a neutral color until the server assigns theirs.
          return t ? { id: t.id, name: t.name, color: t.color } : { id: `pending:${name}`, name, color: "#64748B" };
        });
        patchProjects(qc, (p) => ({ ...p, tags }), projectId);
      },
      errorTitle: "Couldn't update tags",
      invalidate: PROJECT_AND_LISTS,
    }),
  });
}

/** Adds/removes a project from collections (or creates one). Shows the change immediately. */
export function useCollectionMembership(projectId: string) {
  const qc = useQueryClient();
  const keys = PROJECT_AND_LISTS;
  const bumpCount = (collectionId: string, delta: number) =>
    qc.setQueryData<Collection[]>(COLLECTIONS_QUERY_KEY, (list) =>
      list?.map((c) => (c.id === collectionId ? { ...c, project_count: Math.max(0, c.project_count + delta) } : c)),
    );
  const nameOf = (collectionId: string) =>
    qc.getQueryData<Collection[]>(COLLECTIONS_QUERY_KEY)?.find((c) => c.id === collectionId)?.name ?? "collection";

  const add = useMutation({
    mutationFn: (collectionId: string) =>
      apiNoBody("POST", `/collections/${collectionId}/projects`, { project_id: projectId }),
    ...optimisticHandlers<string>(qc, {
      mutationKey: projectMutationKey(projectId),
      keys,
      apply: (collectionId) => {
        const membership = { id: collectionId, name: nameOf(collectionId) };
        patchProjects(
          qc,
          (p) => (p.collections.some((c) => c.id === collectionId) ? p : { ...p, collections: [...p.collections, membership] }),
          projectId,
        );
        bumpCount(collectionId, 1);
      },
      errorTitle: (id) => `Couldn't add to ${nameOf(id)}`,
      invalidate: keys,
    }),
  });

  const remove = useMutation({
    mutationFn: (collectionId: string) => apiNoBody("DELETE", `/collections/${collectionId}/projects/${projectId}`),
    ...optimisticHandlers<string>(qc, {
      mutationKey: projectMutationKey(projectId),
      keys,
      apply: (collectionId) => {
        patchProjects(qc, (p) => ({ ...p, collections: p.collections.filter((c) => c.id !== collectionId) }), projectId);
        // Drop the card from that collection's page right away.
        qc.setQueriesData<{ projects: { id: string }[]; project_count: number }>({ queryKey: ["collection", collectionId] }, (d) =>
          d ? { ...d, projects: d.projects.filter((p) => p.id !== projectId), project_count: Math.max(0, d.project_count - 1) } : d,
        );
        bumpCount(collectionId, -1);
      },
      errorTitle: (id) => `Couldn't remove from ${nameOf(id)}`,
      invalidate: keys,
    }),
  });

  const create = useMutation({
    mutationFn: async (name: string) => {
      const c = await apiJson<Collection>("POST", "/collections", { name });
      await apiNoBody("POST", `/collections/${c.id}/projects`, { project_id: projectId });
    },
    ...optimisticHandlers<string>(qc, {
      mutationKey: projectMutationKey(projectId),
      keys,
      apply: (name) => {
        const temp = { id: `pending:${name}`, name };
        patchProjects(qc, (p) => ({ ...p, collections: [...p.collections, temp] }), projectId);
      },
      errorTitle: (name) => `Couldn't create “${name}”`,
      invalidate: keys,
    }),
  });

  return { add, remove, create };
}

/** Stars/unstars a project. The star flips immediately. */
export function useFavourite(projectId: string, isFavourite: boolean) {
  const qc = useQueryClient();
  const keys = [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY];
  const m = useMutation({
    mutationFn: (on: boolean) => apiNoBody(on ? "PUT" : "DELETE", `/projects/${projectId}/favourite`),
    ...optimisticHandlers<boolean>(qc, {
      mutationKey: projectMutationKey(projectId),
      keys,
      apply: (on) => {
        patchProjects(qc, (p) => ({ ...p, is_favourite: on }), projectId);
        if (!on) {
          // Unstarring on the Favourites page removes the card.
          for (const [key, d] of qc.getQueriesData<{ is_favourites?: boolean; projects: { id: string }[]; project_count: number }>({
            queryKey: ["collection"],
          })) {
            if (d?.is_favourites) {
              qc.setQueryData(key, {
                ...d,
                projects: d.projects.filter((p) => p.id !== projectId),
                project_count: Math.max(0, d.project_count - 1),
              });
            }
          }
        }
      },
      errorTitle: (on) => (on ? "Couldn't add to favourites" : "Couldn't remove from favourites"),
      invalidate: keys,
    }),
  });
  return { on: isFavourite, toggle: () => m.mutate(!isFavourite) };
}

type ProjectPatch = { name?: string; description?: string; main_image_id?: string | null };

/** Edits name / description / cover. Shows the new values immediately. */
export function usePatchProject(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: ProjectPatch) => apiJson<void>("PATCH", `/projects/${projectId}`, patch),
    ...optimisticHandlers<ProjectPatch>(qc, {
      mutationKey: projectMutationKey(projectId),
      apply: (patch) =>
        patchProjects(
          qc,
          (p) => ({
            ...p,
            ...(patch.name !== undefined ? { name: patch.name } : {}),
            ...(patch.description !== undefined ? { description: patch.description } : {}),
            ...(patch.main_image_id !== undefined ? { main_image_id: patch.main_image_id } : {}),
          }),
          projectId,
        ),
      errorTitle: (patch) =>
        patch.main_image_id !== undefined && patch.name === undefined ? "Couldn't change the cover" : "Couldn't save the project",
      invalidate: [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY],
    }),
  });
}

type AssetLike = { id: string; size_bytes: number };

/** Deletes a file. It disappears from the list immediately. */
export function useDeleteAsset(projectId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (assetId: string) => apiNoBody("DELETE", `/projects/${projectId}/assets/${assetId}`),
    ...optimisticHandlers<string>(qc, {
      mutationKey: projectMutationKey(projectId),
      apply: (assetId) => {
        const detail = qc.getQueryData<{ assets: AssetLike[]; main_image_id?: string | null }>(["project", projectId]);
        const size = detail?.assets.find((a) => a.id === assetId)?.size_bytes ?? 0;
        qc.setQueryData<{ assets: AssetLike[]; main_image_id?: string | null }>(["project", projectId], (d) =>
          d
            ? { ...d, assets: d.assets.filter((a) => a.id !== assetId), main_image_id: d.main_image_id === assetId ? null : d.main_image_id }
            : d,
        );
        patchProjects(
          qc,
          (p) => {
            const item = p as typeof p & { asset_count?: number; total_bytes?: number };
            return {
              ...p,
              ...(typeof item.asset_count === "number" ? { asset_count: Math.max(0, item.asset_count - 1) } : {}),
              ...(typeof item.total_bytes === "number" ? { total_bytes: Math.max(0, item.total_bytes - size) } : {}),
              main_image_id: p.main_image_id === assetId ? null : p.main_image_id,
            };
          },
          projectId,
        );
      },
      errorTitle: "Couldn't delete the file",
      invalidate: [...PROJECT_CACHE_KEYS, ["projects", "stats"]],
    }),
  });
}

/**
 * Deletes a project (to the trash, when it's enabled), or with `keepFiles` only removes it
 * from LiMa (the folder stays on disk and sync ignores it). Either way it vanishes from every
 * list immediately; a trashed project gets an Undo toast.
 */
export function useDeleteProject(projectId: string, projectName: string) {
  const qc = useQueryClient();
  type Vars = { keepFiles: boolean };
  return useMutation({
    mutationFn: ({ keepFiles }: Vars) =>
      apiJson<{ trash_id: string | null }>("DELETE", `/projects/${projectId}${keepFiles ? "?files=keep" : ""}`),
    ...optimisticHandlers<Vars>(qc, {
      mutationKey: projectMutationKey(projectId),
      keys: [...PROJECT_CACHE_KEYS, COLLECTIONS_QUERY_KEY],
      apply: () => patchProjects(qc, () => null, projectId),
      errorTitle: ({ keepFiles }) => (keepFiles ? `Couldn't remove “${projectName}”` : `Couldn't delete “${projectName}”`),
      invalidate: [["projects"], ["collection"], COLLECTIONS_QUERY_KEY, ["ignored-folders"], TRASH_KEY],
    }),
    onSuccess: (res) => {
      qc.removeQueries({ queryKey: ["project", projectId] });
      if (res?.trash_id) {
        const trashId = res.trash_id;
        toast(`Moved “${projectName}” to the trash`, {
          duration: 10000,
          action: { label: "Undo", onClick: () => void restoreFromTrash(qc, trashId, projectName) },
        });
      }
    },
  });
}
