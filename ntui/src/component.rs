use std::any::{Any, TypeId};

use crate::element::Element;
use crate::hooks::Hooks;

pub trait Component: 'static {
    type Props: Clone + PartialEq + Default + 'static;
    fn render(props: &Self::Props, hooks: &mut Hooks) -> Element;
}

/// Object-safe erasure of (component type, props).
pub trait AnyComponent {
    fn component_type(&self) -> TypeId;
    fn name(&self) -> &'static str;
    fn render(&self, hooks: &mut Hooks) -> Element;
    fn props_eq(&self, other: &dyn AnyComponent) -> bool;
    fn as_any(&self) -> &dyn Any;
}

pub(crate) struct TypedComponent<C: Component> {
    pub props: C::Props,
    marker: std::marker::PhantomData<fn() -> C>,
}

impl<C: Component> TypedComponent<C> {
    pub fn new(props: C::Props) -> Self {
        TypedComponent {
            props,
            marker: std::marker::PhantomData,
        }
    }
}

impl<C: Component> AnyComponent for TypedComponent<C> {
    fn component_type(&self) -> TypeId {
        TypeId::of::<C>()
    }
    fn name(&self) -> &'static str {
        std::any::type_name::<C>()
    }
    fn render(&self, hooks: &mut Hooks) -> Element {
        C::render(&self.props, hooks)
    }
    fn props_eq(&self, other: &dyn AnyComponent) -> bool {
        other
            .as_any()
            .downcast_ref::<TypedComponent<C>>()
            .is_some_and(|o| o.props == self.props)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
