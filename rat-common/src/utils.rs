use tokio::sync::{Mutex, MutexGuard};

pub fn acquire_free_mutex<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.try_lock()
        .expect("This mutex should never be contended")
}
