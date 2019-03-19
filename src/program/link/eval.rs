use super::{ignore_undefined, EvalContext, RelocTable, Value};

use crate::expr::BinaryOperator;
use crate::model::{Atom, Width};
use crate::program::{Expr, NameDef, NameId, Node, SectionId};

use std::borrow::Borrow;

impl<S: Clone> Expr<S> {
    pub(super) fn eval<R, F>(&self, context: &EvalContext<R, S>, on_undefined: &mut F) -> Value
    where
        R: Borrow<RelocTable>,
        F: FnMut(&S),
    {
        use crate::expr::ExprVariant::*;
        match &self.variant {
            Unary(_, _) => unreachable!(),
            Binary(operator, lhs, rhs) => {
                let lhs = lhs.eval(context, on_undefined);
                let rhs = rhs.eval(context, on_undefined);
                operator.apply(&lhs, &rhs)
            }
            Atom(atom) => atom.eval(context).unwrap_or_else(|()| {
                on_undefined(&self.span);
                Value::Unknown
            }),
        }
    }
}

impl Atom<NameId> {
    fn eval<R: Borrow<RelocTable>, S>(&self, context: &EvalContext<R, S>) -> Result<Value, ()> {
        match self {
            &Atom::Attr(id, _attr) => {
                let name_def = context.program.names.get(id);
                name_def
                    .map(|def| match def {
                        NameDef::Reloc(id) => context.relocs.borrow().get(*id),
                        NameDef::Section(SectionId(section)) => {
                            let reloc = context.program.sections[*section].addr;
                            context.relocs.borrow().get(reloc)
                        }
                    })
                    .ok_or(())
            }
            Atom::Literal(value) => Ok((*value).into()),
            Atom::LocationCounter => Ok(context.location.clone()),
        }
    }
}

impl BinaryOperator {
    fn apply(self, lhs: &Value, rhs: &Value) -> Value {
        match self {
            BinaryOperator::Minus => lhs - rhs,
            BinaryOperator::Multiplication => lhs * rhs,
            BinaryOperator::Plus => lhs + rhs,
            _ => unimplemented!(),
        }
    }
}

impl<S: Clone> Node<S> {
    pub(super) fn size<R: Borrow<RelocTable>>(&self, context: &EvalContext<R, S>) -> Value {
        match self {
            Node::Byte(_) | Node::Embedded(..) => 1.into(),
            Node::Expr(_, width) => width.len().into(),
            Node::LdInlineAddr(_, expr) => match expr.eval(context, &mut ignore_undefined) {
                Value::Range { min, .. } if min >= 0xff00 => 2.into(),
                Value::Range { max, .. } if max < 0xff00 => 3.into(),
                _ => Value::Range { min: 2, max: 3 },
            },
            Node::Symbol(..) => 0.into(),
        }
    }
}

impl Width {
    fn len(self) -> i32 {
        match self {
            Width::Byte => 1,
            Width::Word => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Atom, Attr};
    use crate::program::link::{EvalContext, RelocTable, Value};
    use crate::program::{
        Constraints, NameDef, NameId, NameTable, Program, RelocId, Section, SectionId,
    };

    #[test]
    fn eval_section_addr() {
        let addr = 0x0100;
        let program = Program::<()> {
            sections: vec![Section {
                constraints: Constraints { addr: None },
                addr: RelocId(0),
                size: RelocId(1),
                items: vec![],
            }],
            names: NameTable(vec![Some(NameDef::Section(SectionId(0)))]),
            relocs: 2,
        };
        let relocs = RelocTable(vec![addr.into(), 0.into()]);
        let context = EvalContext {
            program: &program,
            relocs: &relocs,
            location: Value::Unknown,
        };
        assert_eq!(
            Atom::Attr(NameId(0), Attr::Addr).eval(&context),
            Ok(addr.into())
        )
    }
}