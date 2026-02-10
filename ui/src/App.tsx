import { useState } from 'react'
import { Routes, Route, Navigate, Link } from "react-router-dom";
import './App.css'
import { ThemeToggle } from "@/components/theme-toggle";
import { ProjectsPage } from "@/pages/projects";
import { ProjectDetailPage } from "@/pages/project-detail";

function App() {
  const [count, setCount] = useState(0)

  return (
    <div className="min-h-screen">
      <header className="border-b">
        <div className="flex items-center justify-between p-4">
          <Link to="/" className="font-semibold">LIMA</Link>
          <ThemeToggle />
        </div>
      </header>

      <Routes>
        <Route path="/" element={<ProjectsPage />} />
        <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </div>
  )
}

export default App
