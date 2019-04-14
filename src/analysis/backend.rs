use crate::model::{Atom, BinaryOperator, Expr, ExprItem, ExprOperator, Item};
use crate::span::Source;

#[cfg(test)]
pub use mock::*;

pub trait HasValue<S: Clone> {
    type Value: Source<Span = S>;
}

pub trait AllocName<S: Clone> {
    type Name: Clone;

    fn alloc_name(&mut self, span: S) -> Self::Name;
}

pub trait MkValue<T, S: Clone> {
    fn mk_value(&mut self, x: T, span: S);
}

pub trait ApplyBinaryOperator<S: Clone> {
    fn apply_binary_operator(&mut self, operator: (BinaryOperator, S));
}

pub trait PartialBackend<S>
where
    S: Clone,
    Self: HasValue<S>,
{
    fn emit_item(&mut self, item: Item<Self::Value>);
    fn reserve(&mut self, bytes: Self::Value);
    fn set_origin(&mut self, origin: Self::Value);
}

pub trait StartSection<N, S> {
    fn start_section(&mut self, name: (N, S));
}

pub struct LocationCounter;

impl<N> From<LocationCounter> for Atom<N> {
    fn from(_: LocationCounter) -> Self {
        Atom::LocationCounter
    }
}

pub trait ValueBuilder<N, S: Clone>:
    MkValue<LocationCounter, S> + MkValue<i32, S> + MkValue<N, S> + ApplyBinaryOperator<S>
{
}

impl<T, N, S: Clone> ValueBuilder<N, S> for T where
    Self: MkValue<LocationCounter, S> + MkValue<i32, S> + MkValue<N, S> + ApplyBinaryOperator<S>
{
}

impl<N, S: Clone> HasValue<S> for Expr<N, S> {
    type Value = Self;
}

impl<T: Into<Atom<N>>, N: Clone, S: Clone> MkValue<T, S> for Expr<N, S> {
    fn mk_value(&mut self, atom: T, span: S) {
        let atom = atom.into();
        self.0.push(ExprItem {
            op: ExprOperator::Atom(atom.clone()),
            op_span: span.clone(),
            expr_span: span.clone(),
        })
    }
}

impl<N, S: Clone> ApplyBinaryOperator<S> for Expr<N, S> {
    fn apply_binary_operator(&mut self, operator: (BinaryOperator, S)) {
        self.0.push(ExprItem {
            op: ExprOperator::Binary(operator.0),
            op_span: operator.1.clone(),
            expr_span: operator.1,
        })
    }
}

pub trait Backend<S>
where
    S: Clone,
    Self: AllocName<S>,
    Self: PartialBackend<S>,
    Self: StartSection<<Self as AllocName<S>>::Name, S>,
    <Self as HasValue<S>>::Value: Default + ValueBuilder<<Self as AllocName<S>>::Name, S>,
{
    fn define_symbol(&mut self, symbol: (Self::Name, S), value: Self::Value);
}

#[cfg(test)]
mod mock {
    use super::*;

    use crate::model::{Atom, Expr};

    use std::cell::RefCell;

    pub struct MockBackend<'a, T> {
        pub log: &'a RefCell<Vec<T>>,
        next_symbol_id: usize,
    }

    #[derive(Debug, PartialEq)]
    pub enum BackendEvent<V: Source> {
        EmitItem(Item<V>),
        Reserve(V),
        SetOrigin(V),
        DefineSymbol((usize, V::Span), V),
        StartSection((usize, V::Span)),
    }

    impl<'a, T> MockBackend<'a, T> {
        pub fn new(log: &'a RefCell<Vec<T>>) -> Self {
            MockBackend {
                log,
                next_symbol_id: 0,
            }
        }
    }

    impl<'a, T, S> Backend<S> for MockBackend<'a, T>
    where
        T: From<BackendEvent<Expr<usize, S>>>,
        S: Clone,
    {
        fn define_symbol(&mut self, symbol: (Self::Name, S), value: Self::Value) {
            self.log
                .borrow_mut()
                .push(BackendEvent::DefineSymbol(symbol, value).into())
        }
    }

    impl From<usize> for Atom<usize> {
        fn from(n: usize) -> Self {
            Atom::Name(n)
        }
    }

    impl<'a, T, S: Clone> HasValue<S> for MockBackend<'a, T> {
        type Value = Expr<usize, S>;
    }

    impl<'a, T, S: Clone> AllocName<S> for MockBackend<'a, T> {
        type Name = usize;

        fn alloc_name(&mut self, _span: S) -> Self::Name {
            let id = self.next_symbol_id;
            self.next_symbol_id += 1;
            id
        }
    }

    impl<'a, T, S> PartialBackend<S> for MockBackend<'a, T>
    where
        T: From<BackendEvent<Expr<usize, S>>>,
        S: Clone,
    {
        fn emit_item(&mut self, item: Item<Self::Value>) {
            self.log
                .borrow_mut()
                .push(BackendEvent::EmitItem(item).into())
        }

        fn reserve(&mut self, bytes: Self::Value) {
            self.log
                .borrow_mut()
                .push(BackendEvent::Reserve(bytes).into())
        }

        fn set_origin(&mut self, origin: Self::Value) {
            self.log
                .borrow_mut()
                .push(BackendEvent::SetOrigin(origin).into())
        }
    }

    impl<'a, T, S> StartSection<usize, S> for MockBackend<'a, T>
    where
        T: From<BackendEvent<Expr<usize, S>>>,
        S: Clone,
    {
        fn start_section(&mut self, name: (usize, S)) {
            self.log
                .borrow_mut()
                .push(BackendEvent::StartSection(name).into())
        }
    }
}
