import { Routes, Route, Navigate } from "react-router-dom";

import { AppHeader, CreateActionsProvider, MobileNav } from "@/components/layout";
import { Toaster } from "@/components/toaster";
import { ShortcutsProvider } from "@/components/shortcuts-provider";
import { ProjectsPage } from "@/pages/projects";
import { ProjectDetailPage } from "@/pages/project-detail";
import { AdminSyncPage } from "@/pages/admin-sync";
import { AdminTrashPage } from "@/pages/admin-trash";
import { AdminDuplicatesPage } from "@/pages/admin-duplicates";
import { AdminTagsPage } from "@/pages/admin-tags";
import { TagsPage } from "@/pages/tags";
import { CollectionsPage } from "@/pages/collections";
import { CollectionDetailPage } from "@/pages/collection-detail";

function App() {
  return (
    <CreateActionsProvider>
      <ShortcutsProvider>
        <div className="min-h-screen pb-20 md:pb-0">
          <AppHeader />
          <Routes>
            <Route path="/" element={<ProjectsPage />} />
            <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
            <Route path="/tags" element={<TagsPage />} />
            <Route path="/collections" element={<CollectionsPage />} />
            <Route path="/collections/:collectionId" element={<CollectionDetailPage />} />
            <Route path="/admin" element={<Navigate to="/admin/sync" replace />} />
            <Route path="/admin/sync" element={<AdminSyncPage />} />
            <Route path="/admin/sync/:runId" element={<AdminSyncPage />} />
            <Route path="/admin/sync/collections/:runId" element={<AdminSyncPage />} />
            <Route path="/admin/backup" element={<Navigate to="/admin/sync" replace />} />
            <Route path="/admin/trash" element={<AdminTrashPage />} />
            <Route path="/admin/duplicates" element={<AdminDuplicatesPage />} />
            <Route path="/admin/tags" element={<AdminTagsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
          <MobileNav />
          <Toaster />
        </div>
      </ShortcutsProvider>
    </CreateActionsProvider>
  );
}

export default App;
