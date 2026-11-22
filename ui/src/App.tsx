import { Routes, Route, Navigate } from "react-router-dom";

import { AppHeader, MobileNav } from "@/components/layout";
import { Toaster } from "@/components/toaster";
import { ProjectsPage } from "@/pages/projects";
import { ProjectDetailPage } from "@/pages/project-detail";
import { AdminSyncPage } from "@/pages/admin-sync";
import { AdminBackupPage } from "@/pages/admin-backup";
import { AdminTrashPage } from "@/pages/admin-trash";
import { AdminDuplicatesPage } from "@/pages/admin-duplicates";
import { TagsPage } from "@/pages/tags";
import { CollectionsPage } from "@/pages/collections";
import { CollectionDetailPage } from "@/pages/collection-detail";

function App() {
  return (
    <div className="min-h-screen pb-20 md:pb-0">
      <AppHeader />
      <Routes>
        <Route path="/" element={<ProjectsPage />} />
        <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
        <Route path="/tags" element={<TagsPage />} />
        <Route path="/collections" element={<CollectionsPage />} />
        <Route path="/collections/:collectionId" element={<CollectionDetailPage />} />
        <Route path="/admin/sync" element={<AdminSyncPage />} />
        <Route path="/admin/sync/:runId" element={<AdminSyncPage />} />
        <Route path="/admin/sync/collections/:runId" element={<AdminSyncPage />} />
        <Route path="/admin/backup" element={<AdminBackupPage />} />
        <Route path="/admin/trash" element={<AdminTrashPage />} />
        <Route path="/admin/duplicates" element={<AdminDuplicatesPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
      <MobileNav />
      <Toaster />
    </div>
  );
}

export default App;
