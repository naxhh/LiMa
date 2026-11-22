import { Routes, Route, Navigate } from "react-router-dom";

import { AppHeader, MobileNav } from "@/components/layout";
import { ProjectsPage } from "@/pages/projects";
import { ProjectDetailPage } from "@/pages/project-detail";
import { AdminSyncPage } from "@/pages/admin-sync";
import { TagsPage } from "@/pages/tags";

function App() {
  return (
    <div className="min-h-screen pb-20 md:pb-0">
      <AppHeader />
      <Routes>
        <Route path="/" element={<ProjectsPage />} />
        <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
        <Route path="/tags" element={<TagsPage />} />
        <Route path="/admin/sync" element={<AdminSyncPage />} />
        <Route path="/admin/sync/:runId" element={<AdminSyncPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
      <MobileNav />
    </div>
  );
}

export default App;
