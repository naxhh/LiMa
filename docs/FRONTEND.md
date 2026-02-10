# LIMA Frontend – Scope & Architecture

Disclaimer: I'm not good and I don't enjoy doing frontend. So this part will probably AI assited for the 90% of it.

## Goals

The frontend for **LIMA** should:

- Be a **Single Page Application (SPA)** consuming the existing REST API
- Require **minimal frontend effort** while still providing a **modern look & feel**
- Support **large datasets** efficiently (infinite scrolling, virtualization)
- Provide **fast, responsive search** with suggestions
- Allow **project and tag management**
- Support **light and dark mode**
- Be deployable **without a separate frontend server** in production
- Be extensible into a **PWA** later

The frontend is intentionally secondary to the backend; it should *work well*, not become a second large project.

---

## Chosen Stack

### Core
- **React + TypeScript**
- **Vite** (fast dev server, simple build)
- **Tailwind CSS** (utility-first, no custom CSS framework to maintain)
- **shadcn/ui** (modern, accessible UI components copied into the repo)

### Data & State
- **TanStack Query**
  - Data fetching & caching
  - Cursor-based infinite queries
  - Automatic retries and invalidation
- **TanStack Virtual**
  - Efficient rendering of long lists (projects, assets)

### Search & UX
- **cmdk**
  - Fast command-palette style search
  - Ideal for project search & tag selection

### Theming
- **next-themes**
  - Light / Dark / System theme support
  - Stored in `localStorage`
  - Minimal setup, no custom theme logic

### API Integration
- **openapi-typescript**
  - Generate fully typed API client from LIMA OpenAPI spec
  - Avoids manual DTO duplication and runtime mismatches

---

## Application Structure

### Pages

- `/`
  - Projects list
  - Search bar with live suggestions
  - Infinite scrolling
  - "New Project" button

- `/projects/:id`
  - Project details
  - Assets list/grid
  - Tags
  - Metadata editing

- `/tags`
  - List tags
  - Create tag dialog

---

## Core UI Flows

### Create Project

Single modal dialog orchestrating multiple API calls:

1. **Upload files**
   - `POST /bundles` (multipart)
2. **Create project metadata**
   - `POST /projects`
3. **Import bundle**
   - `POST /projects/{project_id}/import`
4. **Invalidate caches**
   - Projects list
   - Navigate to project detail page

User does not manually manage bundles.

### Create Tag

Dialog with:
- Name (required)
- Color (optional, auto-generated if omitted)

API:
- `POST /tags`

---

## Infinite Scrolling & Search

- Cursor-based pagination already implemented in backend
- Frontend uses `useInfiniteQuery`
- Lists rendered with `@tanstack/virtual`
- Search input debounced (≈200ms)
- Cached queries ensure instant backspacing behavior

---

## Theming

- Default: system theme
- Toggle button (sun/moon)
- Applies via Tailwind `dark` class
- No page reload required

---

## Deployment Strategy

### Development
- `vite dev` (hot reload)
- Backend and frontend run separately

### Production
- `vite build`
- Static files served by **Axum**
  - `tower_http::services::ServeDir`
  - SPA fallback to `index.html`

No separate frontend server required.

---

## Progressive Web App (Post-v0)

- Planned via `vite-plugin-pwa`
- Offline support
- Installable app
- Deferred until core functionality is complete

---

## Non-goals (v0)

- Server-side rendering
- Complex animations
- Custom design system
- Advanced asset previews (3D rendering, etc.)

---

## Summary

This approach:
- Minimizes frontend effort
- Maximizes reuse of proven libraries
- Produces a clean, modern UI
- Keeps the backend as the source of truth
- Leaves room for future polish without rework
