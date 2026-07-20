use std::rc::Rc;

/// A cloneable, prop-friendly wrapper around a closure, for widgets that
/// need to notify their parent of an event (`Button`'s `on_press`,
/// `Checkbox`'s `on_change`, etc.).
///
/// Compared by pointer identity so it can satisfy `Component::Props: PartialEq`
/// without requiring `T: PartialEq`. A fresh closure built on every render
/// (the common case: `Callback::new(move || ...)` inline in an `element!`
/// call) therefore compares unequal to the previous render's callback, which
/// only means that widget's props-equality fast path never skips a
/// re-render for this field — not a correctness issue.
pub struct Callback<T = ()>(Rc<dyn Fn(T)>);

impl<T> Callback<T> {
    pub fn new(f: impl Fn(T) + 'static) -> Self {
        Callback(Rc::new(f))
    }

    pub fn call(&self, arg: T) {
        (self.0)(arg)
    }
}

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Callback(self.0.clone())
    }
}

impl<T> PartialEq for Callback<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Default for Callback<T> {
    fn default() -> Self {
        Callback(Rc::new(|_| {}))
    }
}

impl<T> std::fmt::Debug for Callback<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Callback(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc as StdRc;

    #[test]
    fn call_invokes_the_wrapped_closure_with_its_argument() {
        let seen = StdRc::new(Cell::new(0));
        let s = seen.clone();
        let cb: Callback<i32> = Callback::new(move |n| s.set(n));
        cb.call(42);
        assert_eq!(seen.get(), 42);
    }

    #[test]
    fn clones_share_identity_for_prop_equality() {
        let cb: Callback = Callback::new(|_| {});
        assert_eq!(cb, cb.clone());
    }

    #[test]
    fn two_independently_constructed_callbacks_are_unequal() {
        let a: Callback = Callback::new(|_| {});
        let b: Callback = Callback::new(|_| {});
        assert_ne!(a, b);
    }

    #[test]
    fn default_is_a_harmless_no_op() {
        let cb: Callback<i32> = Callback::default();
        cb.call(1); // must not panic
    }
}
