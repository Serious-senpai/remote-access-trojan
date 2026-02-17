use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use log::error;
use tokio::sync::{Mutex, SetOnce};
use tokio::task::JoinHandle;

use crate::framework::module::Module;

struct _SubmoduleState {
    pub module: Arc<dyn Module>,
    pub task: Option<JoinHandle<anyhow::Result<()>>>,
}

pub struct ModuleState {
    _stopped: SetOnce<()>,
    _submodules: Mutex<Vec<_SubmoduleState>>,

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
            _submodules: Mutex::new(
                submodules
                    .into_iter()
                    .map(|m| _SubmoduleState {
                        module: m,
                        task: None,
                    })
                    .collect(),
            ),
            running: AtomicBool::new(false),
        })
    }

    pub(crate) async fn wait_until_stopped(&self) {
        let _ = self._stopped.wait().await;
    }

    pub(crate) fn stopped(&self) -> bool {
        self._stopped.get().is_some()
    }

    pub(crate) fn stop(&self) {
        let _ = self._stopped.set(());
    }

    pub async fn add_submodule(&self, module: Arc<dyn Module>) {
        let task = if self.running.load(Ordering::Acquire) {
            let module = module.clone();
            Some(tokio::spawn(async move { module.run().await }))
        } else {
            None
        };

        let mut submodules = self._submodules.lock().await;
        submodules.push(_SubmoduleState { module, task });
    }

    pub(crate) async fn start_all_submodules(&self) {
        let mut submodules = self._submodules.lock().await;
        for submodule in submodules.iter_mut() {
            if submodule.task.is_none() {
                let module = submodule.module.clone();
                submodule
                    .task
                    .replace(tokio::spawn(async move { module.run().await }));
            }
        }
    }

    pub(crate) async fn stop_all_submodules(&self) {
        let mut submodules = self._submodules.lock().await;
        for submodule in submodules.iter() {
            submodule.module.stop();
        }

        for submodule in submodules.iter_mut() {
            if let Some(task) = submodule.task.take()
                && let Ok(Err(e)) = task.await
            {
                error!("Error in submodule: {e}");
            }
        }
    }
}
