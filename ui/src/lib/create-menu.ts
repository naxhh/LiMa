import { createContext, useContext } from "react";

/** Ways the header's `+` menu (and later, keyboard shortcuts) can open a create dialog. */
export type CreateActions = {
  openProject: () => void;
  openImport: () => void;
  openCollection: () => void;
  openTag: () => void;
};

export const CreateActionsContext = createContext<CreateActions | null>(null);

export function useCreateActions(): CreateActions {
  const ctx = useContext(CreateActionsContext);
  if (!ctx) throw new Error("useCreateActions must be used within a CreateActionsProvider");
  return ctx;
}
