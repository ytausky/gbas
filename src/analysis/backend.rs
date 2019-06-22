pub use crate::model::LocationCounter;

use crate::model::{Atom, BinOp, Expr, ExprOp, FnCall, Item, ParamId};
use crate::span::{Source, WithSpan};

#[cfg(test)]
pub use mock::*;

pub trait AllocName<S: Clone> {
    type Name: Clone;

    fn alloc_name(&mut self, span: S) -> Self::Name;
}

pub trait PushOp<T, S: Clone> {
    fn push_op(&mut self, op: T, span: S);
}

pub trait PartialBackend<S: Clone> {
    type Value: Source<Span = S>;

    fn emit_item(&mut self, item: Item<Self::Value>);
    fn reserve(&mut self, bytes: Self::Value);
    fn set_origin(&mut self, origin: Self::Value);
}

pub trait StartSection<N, S> {
    fn start_section(&mut self, name: (N, S));
}

pub trait ValueBuilder<N, S: Clone>:
    PushOp<LocationCounter, S>
    + PushOp<i32, S>
    + PushOp<N, S>
    + PushOp<BinOp, S>
    + PushOp<ParamId, S>
    + PushOp<FnCall, S>
{
}

impl<T, N, S: Clone> ValueBuilder<N, S> for T where
    Self: PushOp<LocationCounter, S>
        + PushOp<i32, S>
        + PushOp<N, S>
        + PushOp<BinOp, S>
        + PushOp<ParamId, S>
        + PushOp<FnCall, S>
{
}

impl<T: Into<Atom<L, N>>, L, N: Clone, S: Clone> PushOp<T, S> for Expr<Atom<L, N>, S> {
    fn push_op(&mut self, atom: T, span: S) {
        self.0.push(ExprOp::Atom(atom.into()).with_span(span))
    }
}

impl<L, N, S: Clone> PushOp<BinOp, S> for Expr<Atom<L, N>, S> {
    fn push_op(&mut self, op: BinOp, span: S) {
        self.0.push(ExprOp::Binary(op).with_span(span))
    }
}

impl<L, N, S: Clone> PushOp<FnCall, S> for Expr<Atom<L, N>, S> {
    fn push_op(&mut self, FnCall(n): FnCall, span: S) {
        self.0.push(ExprOp::FnCall(n).with_span(span))
    }
}

pub trait Finish<S: Clone> {
    type Parent;
    type Value: Source<Span = S>;

    fn finish(self) -> (Self::Parent, Self::Value);
}

pub trait FinishFnDef {
    type Return;

    fn finish_fn_def(self) -> Self::Return;
}

pub trait Backend<S>
where
    S: Clone,
    Self: Sized,
    Self: AllocName<S>,
    Self: PartialBackend<S>,
    Self: StartSection<<Self as AllocName<S>>::Name, S>,
{
    type ImmediateBuilder: AllocName<S, Name = Self::Name>
        + ValueBuilder<Self::Name, S>
        + Finish<S, Parent = Self, Value = Self::Value>;

    type SymbolBuilder: AllocName<S, Name = Self::Name>
        + ValueBuilder<Self::Name, S>
        + FinishFnDef<Return = Self>;

    fn build_immediate(self) -> Self::ImmediateBuilder;
    fn define_symbol(self, name: Self::Name, span: S) -> Self::SymbolBuilder;
}

pub(crate) struct RelocContext<P, B> {
    pub parent: P,
    pub builder: B,
}

impl<P, B: Default> RelocContext<P, B> {
    pub fn new(parent: P) -> Self {
        Self {
            parent,
            builder: Default::default(),
        }
    }
}

macro_rules! impl_push_op_for_reloc_context {
    ($t:ty) => {
        impl<P, B, S> PushOp<$t, S> for RelocContext<P, B>
        where
            B: PushOp<$t, S>,
            S: Clone,
        {
            fn push_op(&mut self, op: $t, span: S) {
                self.builder.push_op(op, span)
            }
        }
    };
}

impl_push_op_for_reloc_context! {LocationCounter}
impl_push_op_for_reloc_context! {i32}
impl_push_op_for_reloc_context! {BinOp}
impl_push_op_for_reloc_context! {ParamId}
impl_push_op_for_reloc_context! {FnCall}

#[cfg(test)]
mod mock {
    use super::*;

    use crate::log::Log;
    use crate::model::Atom;

    type Expr<S> = crate::model::Expr<Atom<LocationCounter, usize>, S>;

    pub(crate) struct MockBackend<T> {
        pub log: Log<T>,
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

    impl<T> MockBackend<T> {
        pub fn new(log: Log<T>) -> Self {
            MockBackend {
                log,
                next_symbol_id: 0,
            }
        }
    }

    impl<T, S> Backend<S> for MockBackend<T>
    where
        T: From<BackendEvent<Expr<S>>>,
        S: Clone,
    {
        type ImmediateBuilder = RelocContext<Self, Expr<S>>;
        type SymbolBuilder = MockSymbolBuilder<Self, usize, S>;

        fn build_immediate(self) -> Self::ImmediateBuilder {
            RelocContext::new(self)
        }

        fn define_symbol(self, name: Self::Name, span: S) -> Self::SymbolBuilder {
            MockSymbolBuilder {
                parent: self,
                name: (name, span),
                expr: Default::default(),
            }
        }
    }

    impl<T, S: Clone> PushOp<usize, S> for RelocContext<MockBackend<T>, Expr<S>> {
        fn push_op(&mut self, op: usize, span: S) {
            self.builder.push_op(op, span)
        }
    }

    impl<T, S: Clone> AllocName<S> for RelocContext<MockBackend<T>, Expr<S>> {
        type Name = usize;

        fn alloc_name(&mut self, span: S) -> Self::Name {
            self.parent.alloc_name(span)
        }
    }

    impl<T, S: Clone> Finish<S> for RelocContext<MockBackend<T>, Expr<S>> {
        type Parent = MockBackend<T>;
        type Value = Expr<S>;

        fn finish(self) -> (Self::Parent, Self::Value) {
            (self.parent, self.builder)
        }
    }

    pub struct MockSymbolBuilder<P, N, S> {
        pub parent: P,
        pub name: (N, S),
        pub expr: crate::model::Expr<Atom<LocationCounter, N>, S>,
    }

    impl<T, P, N, S: Clone> PushOp<T, S> for MockSymbolBuilder<P, N, S>
    where
        crate::model::Expr<Atom<LocationCounter, N>, S>: PushOp<T, S>,
    {
        fn push_op(&mut self, op: T, span: S) {
            self.expr.push_op(op, span)
        }
    }

    impl<T, S> FinishFnDef for MockSymbolBuilder<MockBackend<T>, usize, S>
    where
        T: From<BackendEvent<Expr<S>>>,
        S: Clone,
    {
        type Return = MockBackend<T>;

        fn finish_fn_def(self) -> Self::Return {
            let parent = self.parent;
            parent
                .log
                .push(BackendEvent::DefineSymbol(self.name, self.expr));
            parent
        }
    }

    impl<T, S: Clone> AllocName<S> for MockSymbolBuilder<MockBackend<T>, usize, S> {
        type Name = usize;

        fn alloc_name(&mut self, span: S) -> Self::Name {
            self.parent.alloc_name(span)
        }
    }

    impl<L> From<usize> for Atom<L, usize> {
        fn from(n: usize) -> Self {
            Atom::Name(n)
        }
    }

    impl<T, S: Clone> AllocName<S> for MockBackend<T> {
        type Name = usize;

        fn alloc_name(&mut self, _span: S) -> Self::Name {
            let id = self.next_symbol_id;
            self.next_symbol_id += 1;
            id
        }
    }

    impl<T, S> PartialBackend<S> for MockBackend<T>
    where
        T: From<BackendEvent<Expr<S>>>,
        S: Clone,
    {
        type Value = Expr<S>;

        fn emit_item(&mut self, item: Item<Self::Value>) {
            self.log.push(BackendEvent::EmitItem(item))
        }

        fn reserve(&mut self, bytes: Self::Value) {
            self.log.push(BackendEvent::Reserve(bytes))
        }

        fn set_origin(&mut self, origin: Self::Value) {
            self.log.push(BackendEvent::SetOrigin(origin))
        }
    }

    impl<T, S> StartSection<usize, S> for MockBackend<T>
    where
        T: From<BackendEvent<Expr<S>>>,
        S: Clone,
    {
        fn start_section(&mut self, name: (usize, S)) {
            self.log.push(BackendEvent::StartSection(name))
        }
    }
}
