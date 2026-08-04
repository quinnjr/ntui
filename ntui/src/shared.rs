//! [`Shared`]: a prop payload compared by pointer instead of by value.

use std::sync::Arc;

/// A prop payload compared by pointer identity rather than by value.
///
/// The reconciler decides whether a subtree needs re-rendering by comparing
/// props with `PartialEq` (`props_eq`). That is a good trade for small
/// props, but it inverts for large ones: `Arc<T>`'s own `PartialEq` compares
/// the *pointees*, so passing a list of several hundred items down as a
/// plain `Arc<Vec<Item>>` deep-compares the whole list on every frame —
/// more expensive than the render the comparison exists to skip.
///
/// `Shared<T>` compares with `Arc::ptr_eq` instead, which is both cheaper
/// and, for the usual producer pattern, more accurate: code that rebuilds
/// its payload each tick hands down a fresh allocation, so identity already
/// answers "did this change?" correctly.
///
/// It derefs to `T`, so readers use it like the value it wraps.
///
/// ```
/// use ntui::Shared;
///
/// let items = Shared::new(vec![1, 2, 3]);
/// assert_eq!(items.len(), 3);          // Deref
/// assert_eq!(items, items.clone());    // same allocation
/// assert_ne!(items, Shared::new(vec![1, 2, 3])); // equal contents, different allocation
/// ```
///
/// Two payloads with equal contents but separate allocations compare
/// unequal. That is the intended trade: the cost is an occasional
/// re-render that `props_eq` could in principle have skipped, and the
/// benefit is never paying a deep comparison. If a component's props are
/// small enough that a value comparison is cheap, use the value directly
/// and skip this type.
///
/// # `Default` allocates
///
/// [`Shared::default`] builds a fresh `Arc`, so two defaults never compare
/// equal. Props structs are conventionally
/// `#[derive(Clone, PartialEq, Default)]` and filled with
/// `..Default::default()`, which means a `Shared` field the caller leaves
/// unset gets a *new pointer every render* — `props_eq` then returns
/// `false` unconditionally and the reconciler's subtree short-circuit is
/// permanently defeated for that component, which is the exact cost this
/// type exists to avoid. The same trap applies to
/// [`use_memo`](crate::Hooks::use_memo) deps.
///
/// Either always pass an explicit value, or hoist the empty case into a
/// `use_state`/`use_memo` so the same allocation is reused across renders.
pub struct Shared<T: ?Sized>(Arc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Shared(Arc::new(value))
    }

    /// Unwrap to the underlying `Arc`, for interop with code that wants one.
    pub fn into_arc(self) -> Arc<T> {
        self.0
    }
}

impl<T: ?Sized> Shared<T> {
    /// Whether two handles point at the same allocation — the same test
    /// `PartialEq` performs, named for when the intent is identity rather
    /// than equality.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: ?Sized> From<Arc<T>> for Shared<T> {
    fn from(arc: Arc<T>) -> Self {
        Shared(arc)
    }
}

impl<T: ?Sized> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> AsRef<T> for Shared<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> Clone for Shared<T> {
    /// Bumps the refcount; never copies the payload.
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T: ?Sized> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: ?Sized> Eq for Shared<T> {}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Shared::new(T::default())
    }
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_identity() {
        let a = Shared::new(vec![1, 2, 3]);
        let b = a.clone();

        assert_eq!(a, b);
        assert!(a.ptr_eq(&b));
    }

    #[test]
    fn equal_contents_in_separate_allocations_are_not_equal() {
        // The whole point: comparison must never look at the payload, no
        // matter how large or how identical it happens to be.
        let a = Shared::new(vec![1, 2, 3]);
        let b = Shared::new(vec![1, 2, 3]);

        assert_ne!(a, b);
    }

    #[test]
    fn does_not_require_the_payload_to_be_partial_eq() {
        #[derive(Debug)]
        struct NotComparable(#[allow(dead_code)] u32);

        let a = Shared::new(NotComparable(1));
        let b = a.clone();

        assert!(a.ptr_eq(&b));
    }

    #[test]
    fn derefs_to_the_payload() {
        let s = Shared::new(String::from("hello"));

        assert_eq!(s.len(), 5);
        assert_eq!(&*s, "hello");
    }

    #[test]
    fn default_allocates_a_default_payload() {
        let s: Shared<Vec<u8>> = Shared::default();

        assert!(s.is_empty());
    }

    #[test]
    fn two_defaults_are_not_equal() {
        // Documented footgun: a defaulted props field gets a fresh pointer
        // every render, so `props_eq` never short-circuits that component.
        let a: Shared<Vec<u8>> = Shared::default();
        let b: Shared<Vec<u8>> = Shared::default();

        assert_ne!(a, b);
    }

    #[test]
    fn round_trips_through_arc() {
        let arc = Arc::new(7u32);
        let shared = Shared::from(arc.clone());

        assert!(Arc::ptr_eq(&shared.clone().into_arc(), &arc));
    }

    #[test]
    fn debug_shows_the_payload_not_the_pointer() {
        let s = Shared::new(vec![1, 2]);

        assert_eq!(format!("{s:?}"), "[1, 2]");
    }
}
