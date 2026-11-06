//! One input/render thread and at most one owned worker; workers are never detached.
use super::actions::{self, Output, Task};
use dort_core::context::HostContext;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
};

pub struct Worker {
    handle: Option<JoinHandle<dort_core::Result<Output>>>,
    progress: Receiver<super::task_surfaces::Progress>,
}
impl Worker {
    pub fn start(
        task: Task,
        host: HostContext,
        repository: Option<PathBuf>,
        requested: Vec<String>,
    ) -> std::io::Result<Self> {
        let (sender, progress) = mpsc::sync_channel(32);
        let handle = thread::Builder::new()
            .name("dort-tui-service".into())
            .spawn(move || {
                actions::perform_scoped(task, &host, repository, &requested, |event| {
                    // Dropped/coalesced display updates never interrupt an approved write.
                    let _ = sender.try_send(super::task_surfaces::Progress::from_event(event));
                    Ok(())
                })
            })?;
        Ok(Self {
            handle: Some(handle),
            progress,
        })
    }
    pub fn progress(&self) -> Option<super::task_surfaces::Progress> {
        self.progress.try_iter().last()
    }
    pub fn finished(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| h.is_finished())
    }
    pub fn join(&mut self) -> std::thread::Result<dort_core::Result<Output>> {
        self.handle.take().expect("worker joined once").join()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, Tab};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    #[test]
    fn active_worker_keeps_input_responsive_and_is_joined_on_error_cleanup() {
        let (release, wait) = mpsc::channel();
        let (done, completed) = mpsc::channel();
        let (progress_sender, progress) = mpsc::sync_channel(1);
        let worker = Worker {
            handle: Some(thread::spawn(move || {
                progress_sender
                    .send(super::super::task_surfaces::Progress {
                        transaction_id: "test-transaction".into(),
                        boundary: "Durable checkpoint",
                        sequence: 1,
                    })
                    .unwrap();
                wait.recv().unwrap();
                done.send(()).unwrap();
                Ok(Output::Completed(actions::Completion {
                    operation: "test",
                    transaction_id: None,
                    repository: None,
                }))
            })),
            progress,
        };
        let mut app = App::new(false);
        app.start(true);
        app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), 80, 24);
        assert!(app.tab == Tab::Files);
        app.key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            80,
            24,
        );
        assert!(app.exit_requested && !app.quit);
        assert!(!worker.finished());
        // Simulate rendering/input failure: dropping the owned worker must join it.
        let cleanup = thread::spawn(move || drop(worker));
        assert!(completed.try_recv().is_err());
        release.send(()).unwrap();
        cleanup.join().unwrap();
        completed.recv().unwrap();
    }
}
