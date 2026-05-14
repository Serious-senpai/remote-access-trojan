use core::slice;

pub struct DropGuard<T, F>
where
    F: FnOnce(T),
{
    argument: Option<T>,
    drop_fn: Option<F>,
}

impl<T, F> DropGuard<T, F>
where
    F: FnOnce(T),
{
    pub fn new(argument: T, drop_fn: F) -> Self {
        Self {
            argument: Some(argument),
            drop_fn: Some(drop_fn),
        }
    }
}

impl<T, F> Drop for DropGuard<T, F>
where
    F: FnOnce(T),
{
    fn drop(&mut self) {
        if let Some(argument) = self.argument.take()
            && let Some(drop_fn) = self.drop_fn.take()
        {
            drop_fn(argument);
        }
    }
}

/// # Safety
/// `start` and `end` must be valid pointers, and the memory from `start` to `end` must be valid
/// for `'static` lifetime.
///
/// If `end` is before `start`, the returned slice will be empty.
pub unsafe fn get_function_code(start: *const u8, end: *const u8) -> &'static [u8] {
    let start_addr = start as usize;
    let end_addr = end as usize;

    unsafe { slice::from_raw_parts(start, end_addr.saturating_sub(start_addr)) }
}
