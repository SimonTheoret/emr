use eyre::{Context, Result, bail};
use notify::event::RemoveKind;
use notify::{
    ErrorKind, Event, EventKind, Result as EventResult, Watcher, event::CreateKind,
    recommended_watcher,
};
use ron::from_str;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use tracing::{info, warn};

use crate::ir::Mock;

trait SendWatcher: Watcher + Send {}
impl<W> SendWatcher for W where W: Watcher + Send {}

/// It is possible to have a racing condition around the filesystem. For
/// example, if a user delete a file and (quickly) recreate the same file, it is
/// possible we have processed a request before the file is created.
/// Time of Check to Time of Use (TOCTOU)
pub struct MockFileStorage {
    dir: PathBuf,
    mocks: HashMap<PathBuf, Mock>,
    events_rx: Receiver<EventResult<Event>>,
    serialization_ext: &'static str,
    /// We keep the watcher inside the storage to avoid it from being dropped.
    _watcher: Box<dyn SendWatcher>,
}

impl MockFileStorage {
    pub fn new<P: Into<PathBuf>>(mock_dir: P, serialization_ext: &'static str) -> Result<Self> {
        let dir = mock_dir.into();
        if !dir.exists() {
            std::fs::create_dir(&dir)?;
        }
        let (tx, rx) = channel();
        let mut watcher =
            recommended_watcher(tx).wrap_err_with(|| "Error when creating the watcher")?;
        watcher.watch(dir.as_path(), notify::RecursiveMode::Recursive)?;
        let files = Self::load_all_files(&dir, serialization_ext)?;
        info!("Storage backend initialized");
        Ok(Self {
            dir,
            mocks: files,
            events_rx: rx,
            serialization_ext,
            _watcher: Box::new(watcher),
        })
    }
    pub fn files(&mut self) -> Result<&HashMap<PathBuf, Mock>> {
        let events: Vec<_> = self.events_rx.try_iter().collect();
        for ev in events {
            match ev {
                Err(err) => match err.kind {
                    ErrorKind::InvalidConfig(config) => {
                        bail!("Invalid config: {:#?}", config)
                    }
                    _ => {
                        warn!("Error when loading mocks: {err}");
                        continue;
                    }
                },
                Ok(ev) => self.handle_event(ev)?,
            }
        }
        Ok(&self.mocks)
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        if event.need_rescan() {
            self.reload_all_mocks()
        } else {
            let Event { kind, paths, .. } = event;
            match kind {
                EventKind::Access(_)
                | EventKind::Create(CreateKind::Folder)
                | EventKind::Remove(RemoveKind::Folder)
                | EventKind::Modify(notify::event::ModifyKind::Metadata(_)) => Ok(()),

                EventKind::Create(_) | EventKind::Modify(_) => self.update_mocks_at_paths(paths),
                EventKind::Remove(_) => self.remove_mocks_at_paths(paths),
                EventKind::Any | EventKind::Other => self.reload_all_mocks(),
            }
        }
    }
    fn reload_all_mocks(&mut self) -> Result<()> {
        info!("Need to reload all mock from storage");
        let mocks = Self::load_all_files(self.dir.as_path(), self.serialization_ext)?;
        self.mocks = mocks;
        Ok(())
    }

    fn update_mocks_at_paths(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        info!(
            "Need to reload all mock from storage at paths: {:#?}",
            paths
        );

        for p in paths.into_iter().filter(|p| {
            p.extension().is_some_and(|ext| {
                *ext == *self.serialization_ext && p.try_exists().is_ok_and(|b| b)
            })
        }) {
            info!("Reloading modified mock at path '{:#?}'", &p);
            let mock = load_mock(p.as_path())?;
            self.mocks.insert(p, mock);
        }
        Ok(())
    }

    fn remove_mocks_at_paths(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        info!(
            "Need to remove all mock from storage at paths: {:#?}",
            paths
        );
        for p in paths.into_iter().filter(|p| {
            p.extension().is_some_and(|ext| {
                *ext == *self.serialization_ext && p.try_exists().is_ok_and(|b| b)
            })
        }) {
            info!("Reloading modified mock at path '{:#?}'", &p);
            self.mocks.remove(&p);
        }
        Ok(())
    }

    fn load_all_files<P: AsRef<Path>, S: AsRef<str>>(
        dir: P,
        serialization_ext: S,
    ) -> Result<HashMap<PathBuf, Mock>> {
        let walker = dir_walker::Walker::new(dir).skip_dotted().walk_dir()?;
        walker
            .into_iter()
            .filter_map(|e_item| {
                let path_buf = e_item.dirent.path();
                if path_buf.extension().is_some_and(|ext| {
                    ext == serialization_ext.as_ref() && path_buf.try_exists().is_ok_and(|b| b)
                }) {
                    Some(path_buf)
                } else {
                    None
                }
            })
            .map(|pb| {
                let mock: Mock = load_mock(pb.as_path())?;
                Ok((pb, mock))
            })
            .collect()
    }
}

fn load_mock<P: AsRef<Path>>(path: P) -> Result<Mock> {
    Ok(from_str(&std::fs::read_to_string(path)?)?)
}
