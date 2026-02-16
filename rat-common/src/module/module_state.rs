use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use log::error;
use tokio::sync::{Mutex, SetOnce};
use tokio::task::JoinHandle;

use crate::module::module::Module;

pub struct ModuleState {
    _stopped: SetOnce<()>,
    _submodules: Mutex<Vec<(Arc<dyn Module>, Option<JoinHandle<anyhow::Result<()>>>)>>,

    pub(crate) running: AtomicBool,
}

impl ModuleState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            _stopped: SetOnce::new(),
            _submodules: Mutex::new(vec![]),
            running: AtomicBool::new(false),
        })
    }

    pub fn new_with_submodules(submodules: Vec<Arc<dyn Module>>) -> Arc<Self> {
        Arc::new(Self {
            _stopped: SetOnce::new(),
            _submodules: Mutex::new(submodules.into_iter().map(|m| (m, None)).collect()),
            running: AtomicBool::new(false),
        })
    }

    pub async fn wait_until_stopped(&self) {
        let _ = self._stopped.wait().await;
    }

    pub fn stopped(&self) -> bool {
        self._stopped.get().is_some()
    }

    pub fn stop(&self) {
        let _ = self._stopped.set(());
    }

    pub async fn add_submodule(&self, module: Arc<dyn Module>) {
        let mut submodules = self._submodules.lock().await;
        let task = if self.running.load(Ordering::Acquire) {
            let module = module.clone();
            Some(tokio::spawn(async move { module.run().await }))
        } else {
            None
        };

        submodules.push((module, task));
    }

    pub async fn start_all_submodules(&self) {
        let mut submodules = self._submodules.lock().await;
        for (module, task) in submodules.iter_mut() {
            if task.is_none() {
                let module = module.clone();
                *task = Some(tokio::spawn(async move { module.run().await }));
            }
        }
    }

    pub async fn stop_all_submodules(&self) {
        let mut submodules = self._submodules.lock().await;
        for (module, _) in submodules.iter() {
            module.stop();
        }

        for (_, task) in submodules.iter_mut() {
            if let Some(task) = task.take() {
                if let Ok(Err(e)) = task.await {
                    error!("Error in submodule: {e}");
                }
            }
        }
    }
}
