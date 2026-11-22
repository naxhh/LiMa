//! Best-effort filesystem watcher. Collects the project folders touched by
//! filesystem events and, once things are quiet, runs an incremental sync
//! limited to those folders. Manual and full syncs remain the source of truth.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use super::{StartSyncError, SyncManager, SyncMode, SyncReason, now};

#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Wait this long without new events before syncing (lets copies finish).
    pub quiet_period: Duration,
    /// Sync anyway after this long, even if events keep arriving.
    pub max_delay: Duration,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self { quiet_period: Duration::from_secs(5), max_delay: Duration::from_secs(60) }
    }
}

/// Folders affected by one event, and whether the whole library must be rescanned.
#[derive(Debug, Default, PartialEq)]
struct Affected {
    folders: HashSet<String>,
    rescan: bool,
}

fn affected(root: &Path, event: &Event) -> Affected {
    let mut out = Affected::default();
    if event.need_rescan() {
        out.rescan = true;
        return out;
    }
    // Reads (including our own scans and hashing) are not changes.
    if matches!(event.kind, EventKind::Access(_)) {
        return out;
    }
    for path in &event.paths {
        let Ok(rel) = path.strip_prefix(root) else { continue };
        match rel.components().next() {
            Some(Component::Normal(name)) => {
                if let Some(name) = name.to_str() {
                    // Hidden folders are never projects; hidden files inside a
                    // project are skipped by the scan, but check the full path.
                    let hidden = rel
                        .components()
                        .any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.')));
                    if !hidden {
                        out.folders.insert(name.to_string());
                    }
                }
            }
            // The library root itself changed (removed, moved...): check everything.
            None if matches!(event.kind, EventKind::Remove(_)) => out.rescan = true,
            _ => {}
        }
    }
    out
}

pub(super) fn spawn(manager: SyncManager, config: WatcherConfig) {
    manager.update_watcher_status(|s| s.enabled = true);

    let fail = |manager: &SyncManager, msg: String| {
        tracing::warn!("Library watcher disabled: {msg}");
        manager.update_watcher_status(|s| {
            s.active = false;
            s.error = Some(msg);
        });
    };

    let root = match manager.library_root().canonicalize() {
        Ok(root) => root,
        Err(e) => return fail(&manager, format!("Library folder not available: {e}")),
    };

    let (tx, rx) = mpsc::unbounded_channel();
    let mut os_watcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => return fail(&manager, format!("Could not create watcher: {e}")),
    };
    if let Err(e) = os_watcher.watch(&root, RecursiveMode::Recursive) {
        // Typically the inotify watch limit (fs.inotify.max_user_watches) on huge libraries.
        return fail(&manager, format!("Could not watch {}: {e}", root.display()));
    }

    manager.set_os_watcher(Some(os_watcher));
    manager.update_watcher_status(|s| {
        s.active = true;
        s.error = None;
    });
    tracing::info!("Watching {} for changes", root.display());

    tokio::spawn(event_loop(manager, root, rx, config));
}

async fn event_loop(
    manager: SyncManager,
    root: PathBuf,
    mut rx: mpsc::UnboundedReceiver<notify::Result<Event>>,
    config: WatcherConfig,
) {
    let mut pending: HashSet<String> = HashSet::new();
    let mut rescan = false;
    let mut first_event: Option<Instant> = None;
    let mut last_event: Option<Instant> = None;
    // A run was in progress when we wanted to start ours.
    let mut blocked = false;

    loop {
        let deadline = match (blocked, first_event, last_event) {
            (false, Some(first), Some(last)) => Some((last + config.quiet_period).min(first + config.max_delay)),
            _ => None,
        };

        tokio::select! {
            msg = rx.recv() => {
                let change = match msg {
                    None => {
                        manager.update_watcher_status(|s| {
                            s.active = false;
                            s.error = Some("Watcher stopped".into());
                        });
                        return;
                    }
                    Some(Ok(event)) => affected(&root, &event),
                    Some(Err(e)) => {
                        tracing::warn!("Library watcher error: {e}");
                        manager.update_watcher_status(|s| s.error = Some(e.to_string()));
                        Affected { rescan: true, ..Default::default() }
                    }
                };
                if change.folders.is_empty() && !change.rescan {
                    continue;
                }

                pending.extend(change.folders);
                rescan |= change.rescan;
                let at = Instant::now();
                first_event.get_or_insert(at);
                last_event = Some(at);

                let folders = sorted(&pending);
                manager.update_watcher_status(|s| {
                    s.pending_folders = folders;
                    s.full_rescan_pending = rescan;
                    s.last_event_at = Some(now());
                });
            }

            _ = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                let folders = if rescan { None } else { Some(sorted(&pending)) };
                match manager.start_scoped(SyncMode::Incremental, SyncReason::Watcher, folders).await {
                    Ok(run) => {
                        tracing::debug!("Watcher started sync run {}", run.id);
                        pending.clear();
                        rescan = false;
                        first_event = None;
                        last_event = None;
                        manager.update_watcher_status(|s| {
                            s.pending_folders.clear();
                            s.full_rescan_pending = false;
                            s.last_triggered_at = Some(now());
                        });
                    }
                    // Keep collecting; try again once that run is over.
                    Err(StartSyncError::AlreadyRunning { .. }) => blocked = true,
                    Err(e) => {
                        tracing::warn!("Watcher could not start a sync: {e}");
                        manager.update_watcher_status(|s| s.error = Some(e.to_string()));
                        last_event = Some(Instant::now()); // retry after another quiet period
                        first_event = last_event;
                    }
                }
            }

            _ = manager.wait_idle(), if blocked => {
                blocked = false;
                // Give the previous run's dust a moment to settle.
                let at = Instant::now();
                first_event = Some(at);
                last_event = Some(at);
            }
        }
    }
}

fn sorted(set: &HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = set.iter().cloned().collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};

    fn ev(kind: EventKind, paths: &[&str]) -> Event {
        let mut e = Event::new(kind);
        for p in paths {
            e = e.add_path(PathBuf::from(p));
        }
        e
    }

    fn folders(a: &Affected) -> Vec<String> {
        sorted(&a.folders)
    }

    #[test]
    fn maps_paths_to_top_level_folders() {
        let root = Path::new("/lib");
        let a = affected(root, &ev(EventKind::Create(CreateKind::File), &["/lib/benchy/parts/hull.stl"]));
        assert_eq!(folders(&a), ["benchy"]);

        let rename = ev(EventKind::Modify(ModifyKind::Name(RenameMode::Both)), &["/lib/old", "/lib/new"]);
        assert_eq!(folders(&affected(root, &rename)), ["new", "old"]);
    }

    #[test]
    fn ignores_reads_hidden_and_outside_paths() {
        let root = Path::new("/lib");
        assert_eq!(affected(root, &ev(EventKind::Access(AccessKind::Any), &["/lib/benchy/a.stl"])), Affected::default());
        assert_eq!(affected(root, &ev(EventKind::Create(CreateKind::File), &["/lib/.trash/x"])), Affected::default());
        assert_eq!(affected(root, &ev(EventKind::Create(CreateKind::File), &["/lib/benchy/.a.swp"])), Affected::default());
        assert_eq!(affected(root, &ev(EventKind::Create(CreateKind::File), &["/elsewhere/x"])), Affected::default());
    }

    #[test]
    fn root_removal_triggers_rescan() {
        let a = affected(Path::new("/lib"), &ev(EventKind::Remove(RemoveKind::Folder), &["/lib"]));
        assert!(a.rescan);
    }
}
