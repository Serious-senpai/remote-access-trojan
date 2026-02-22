use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use log::{error, warn};
use slotmap::{DefaultKey, SlotMap};
use tokio::sync::{Mutex, SetOnce, watch};
use tokio::task::JoinHandle;

use crate::framework::module::Module;

struct _SubmoduleState {
    pub module: Arc<dyn Module>,
    pub task: Option<JoinHandle<anyhow::Result<()>>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SubmoduleHandle(DefaultKey);

pub struct ModuleState {
    _stopped: SetOnce<()>,
    _submodules: Mutex<SlotMap<DefaultKey, _SubmoduleState>>,
    _submodules_removed: watch::Sender<SubmoduleHandle>,

    pub(crate) running: AtomicBool,
}

impl ModuleState {
    pub fn new() -> Arc<Self> {
        Self::new_with_submodules(vec![])
    }

    pub fn new_with_submodules(submodules: Vec<Arc<dyn Module>>) -> Arc<Self> {
        let send = watch::channel(SubmoduleHandle::default()).0;

        let mut modules = SlotMap::with_capacity(submodules.len());
        for module in submodules {
            modules.insert(_SubmoduleState { module, task: None });
        }

        Arc::new(Self {
            _stopped: SetOnce::new(),
            _submodules: Mutex::new(modules),
            _submodules_removed: send,
            running: AtomicBool::new(false),
        })
    }

    pub(crate) fn subscribe_submodules_removed(&self) -> watch::Receiver<SubmoduleHandle> {
        self._submodules_removed.subscribe()
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

    async fn _execute_submodule(
        self: Arc<Self>,
        key: DefaultKey,
        module: Arc<dyn Module>,
    ) -> anyhow::Result<()> {
        let result = module.run().await;
        let mut submodules = self._submodules.lock().await;
        submodules.remove(key);
        let _ = self._submodules_removed.send(SubmoduleHandle(key));
        result
    }

    pub(crate) async fn add_submodule(self: Arc<Self>, module: Arc<dyn Module>) -> SubmoduleHandle {
        let mut submodules = self._submodules.lock().await;
        let module_c = module.clone();

        let key = submodules.insert_with_key(|key| _SubmoduleState {
            module,
            task: if self.running.load(Ordering::Acquire) {
                let self_c = self.clone();
                Some(tokio::spawn(async move {
                    self_c._execute_submodule(key, module_c).await
                }))
            } else {
                None
            },
        });

        SubmoduleHandle(key)
    }

    pub(crate) async fn get_submodule(&self, handle: SubmoduleHandle) -> Option<Arc<dyn Module>> {
        let submodules = self._submodules.lock().await;
        submodules.get(handle.0).map(|s| s.module.clone())
    }

    pub(crate) async fn remove_submodule(&self, handle: SubmoduleHandle) -> anyhow::Result<()> {
        let removed = {
            let mut submodules = self._submodules.lock().await;
            submodules.remove(handle.0)
        };

        match removed {
            Some(submodule) => {
                submodule.module.stop();
                if let Some(task) = submodule.task {
                    task.await??;
                }
            }
            None => {
                warn!(
                    "Attempted to remove non-existent submodule with key {:?}",
                    handle.0
                );
            }
        }

        Ok(())
    }

    pub(crate) async fn start_all_submodules(self: Arc<Self>) {
        let mut submodules = self._submodules.lock().await;
        for (key, submodule) in submodules.iter_mut() {
            if submodule.task.is_none() {
                let self_c = self.clone();
                let module = submodule.module.clone();

                submodule.task.replace(tokio::spawn(async move {
                    self_c._execute_submodule(key, module).await
                }));
            }
        }
    }

    pub(crate) async fn stop_all_submodules(&self) {
        let tasks = {
            let mut submodules = self._submodules.lock().await;
            for (_, submodule) in submodules.iter() {
                submodule.module.stop();
            }

            submodules
                .iter_mut()
                .filter_map(|(_, submodule)| submodule.task.take())
                .collect::<Vec<_>>()
        };

        for task in tasks {
            if let Ok(Err(e)) = task.await {
                error!("Error in submodule: {e}");
            }
        }
    }
}
