use std::sync::{Arc, Mutex, MutexGuard};

/// Clonable shared cell for smuggling handles/logs out of components in tests.
/// PartialEq is pointer identity so it can sit in Props without defeating props_eq.
pub(crate) struct Shared<T>(pub Arc<Mutex<T>>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}
impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Shared(Arc::new(Mutex::new(T::default())))
    }
}
impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl<T> Shared<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}
