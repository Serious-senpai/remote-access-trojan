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
