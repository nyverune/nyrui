//! Reactive context trait enabling signals to read and write across diverse execution environments.

use super::ReactiveGraph;

/// Trait for execution contexts providing access to a [`ReactiveGraph`].
///
/// Implemented for [`ReactiveGraph`] directly, references (`&T` and `&mut T`),
/// as well as component lifecycle contexts ([`crate::component::ComponentContext`],
/// [`crate::component::RenderContext`]).
pub trait ReactiveContext {
    /// Returns a reference to the active [`ReactiveGraph`].
    fn reactive_graph(&self) -> &ReactiveGraph;
}

impl ReactiveContext for ReactiveGraph {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        self
    }
}

impl ReactiveContext for std::rc::Rc<ReactiveGraph> {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        self
    }
}

impl<C: ReactiveContext + ?Sized> ReactiveContext for &C {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        (**self).reactive_graph()
    }
}

impl<C: ReactiveContext + ?Sized> ReactiveContext for &mut C {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        (**self).reactive_graph()
    }
}
