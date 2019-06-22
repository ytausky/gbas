use super::{Ident, Params, PushOp};

use crate::analysis::backend::{Finish, FinishFnDef, LocationCounter, Name};
use crate::diag::Diagnostics;
use crate::model::{BinOp, FnCall, ParamId};

pub(super) struct ParamsAdapter<'a, P, R, S> {
    parent: P,
    params: &'a Params<R, S>,
}

impl<'a, P, R, S> ParamsAdapter<'a, P, R, S> {
    pub fn new(parent: P, params: &'a Params<R, S>) -> Self {
        Self { parent, params }
    }
}

impl<'a, P, R, S> PushOp<Name<Ident<R>>, S> for ParamsAdapter<'a, P, R, S>
where
    P: PushOp<Name<Ident<R>>, S> + PushOp<ParamId, S>,
    R: Eq,
    S: Clone,
{
    fn push_op(&mut self, Name(ident): Name<Ident<R>>, span: S) {
        let param = self
            .params
            .0
            .iter()
            .position(|param| param.name == ident.name)
            .map(ParamId);
        if let Some(id) = param {
            self.parent.push_op(id, span)
        } else {
            self.parent.push_op(Name(ident), span)
        }
    }
}

macro_rules! impl_push_op_for_params_adapter {
    ($t:ty) => {
        impl<'a, P, R, S> PushOp<$t, S> for ParamsAdapter<'a, P, R, S>
        where
            P: PushOp<$t, S>,
            S: Clone,
        {
            fn push_op(&mut self, op: $t, span: S) {
                self.parent.push_op(op, span)
            }
        }
    };
}

impl_push_op_for_params_adapter! {LocationCounter}
impl_push_op_for_params_adapter! {i32}
impl_push_op_for_params_adapter! {BinOp}
impl_push_op_for_params_adapter! {FnCall}

impl<'a, P, R, S> Finish<S> for ParamsAdapter<'a, P, R, S>
where
    P: Finish<S>,
    S: Clone,
{
    type Parent = P::Parent;
    type Value = P::Value;

    fn finish(self) -> (Self::Parent, Self::Value) {
        self.parent.finish()
    }
}

impl<'a, P, R, S> FinishFnDef for ParamsAdapter<'a, P, R, S>
where
    P: FinishFnDef,
    S: Clone,
{
    type Return = P::Return;

    fn finish_fn_def(self) -> Self::Return {
        self.parent.finish_fn_def()
    }
}

delegate_diagnostics! {
    {'a, P: Diagnostics<S>, R, S}, ParamsAdapter<'a, P, R, S>, {parent}, P, S
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::model::{Atom, ParamId};

    type Expr<N, S> = crate::model::Expr<Atom<LocationCounter, N>, S>;

    #[test]
    fn translate_param() {
        let name: Ident<_> = "param".into();
        let builder: Expr<_, _> = Default::default();
        let params = (vec![name.clone()], vec![()]);
        let mut adapter = ParamsAdapter::new(builder, &params);
        adapter.push_op(Name(name), ());
        let mut expected: Expr<_, _> = Default::default();
        expected.push_op(ParamId(0), ());
        assert_eq!(adapter.parent, expected)
    }

    #[test]
    fn pass_through_non_param() {
        let param: Ident<_> = "param".into();
        let builder: Expr<_, _> = Default::default();
        let params = (vec![param.clone()], vec![()]);
        let mut adapter = ParamsAdapter::new(builder, &params);
        let unrelated = Name(Ident::from("ident"));
        adapter.push_op(unrelated.clone(), ());
        let mut expected: Expr<_, _> = Default::default();
        expected.push_op(unrelated, ());
        assert_eq!(adapter.parent, expected)
    }
}
