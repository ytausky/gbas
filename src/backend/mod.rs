pub use crate::backend::object::link;
pub use crate::backend::object::ObjectBuilder;

use crate::backend::{
    lowering::Lower,
    object::{Node, Object},
};
use crate::expr::{Expr, ExprVariant};
use crate::instruction::Instruction;
use crate::span::{Source, Span};
use std::fmt::Debug;
use std::marker::PhantomData;

mod lowering;
mod object;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    Byte,
    Word,
}

impl Width {
    fn len(self) -> i32 {
        match self {
            Width::Byte => 1,
            Width::Word => 2,
        }
    }
}

pub trait HasValue
where
    Self: Span,
{
    type Value: Source<Span = Self::Span>;
}

pub trait BuildValue<'a, V: Source> {
    type Builder: ValueBuilder<V>;
    fn build_value(&'a mut self) -> Self::Builder;
}

pub trait ValueBuilder<V: Source>
where
    Self: Span<Span = V::Span>,
{
    fn location(&mut self, span: V::Span) -> V;
    fn number(&mut self, number: (i32, V::Span)) -> V;
    fn symbol(&mut self, symbol: (String, V::Span)) -> V;

    fn apply_binary_operator(
        &mut self,
        operator: (BinaryOperator, V::Span),
        left: V,
        right: V,
    ) -> V;
}

pub trait Backend<S: Clone + Debug + PartialEq>
where
    Self: HasValue<Span = S>,
    for<'a> Self: BuildValue<'a, <Self as HasValue>::Value>,
{
    type Object;
    fn define_symbol(&mut self, symbol: (impl Into<String>, S), value: Self::Value);
    fn emit_item(&mut self, item: Item<Self::Value>);
    fn into_object(self) -> Self::Object;
    fn set_origin(&mut self, origin: Self::Value);
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item<V: Source> {
    Data(V, Width),
    Instruction(Instruction<V>),
}

pub type RelocExpr<S> = Expr<RelocAtom, Empty, BinaryOperator, S>;

#[derive(Clone, Debug, PartialEq)]
pub enum Empty {}

#[derive(Clone, Debug, PartialEq)]
pub enum RelocAtom {
    Literal(i32),
    LocationCounter,
    Symbol(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BinaryOperator {
    Minus,
    Plus,
}

impl<S> From<i32> for ExprVariant<RelocAtom, Empty, BinaryOperator, S> {
    fn from(n: i32) -> Self {
        ExprVariant::Atom(RelocAtom::Literal(n))
    }
}

#[cfg(test)]
impl<T: Into<ExprVariant<RelocAtom, Empty, BinaryOperator, ()>>> From<T> for RelocExpr<()> {
    fn from(variant: T) -> Self {
        Expr {
            variant: variant.into(),
            span: (),
        }
    }
}

pub struct BinaryObject {
    sections: Vec<BinarySection>,
}

impl BinaryObject {
    pub fn into_rom(self) -> Rom {
        let mut data: Vec<u8> = Vec::new();
        for chunk in self.sections {
            if !chunk.data.is_empty() {
                let end = chunk.origin + chunk.data.len();
                if data.len() < end {
                    data.resize(end, 0x00)
                }
                data[chunk.origin..end].copy_from_slice(&chunk.data)
            }
        }
        if data.len() < MIN_ROM_LEN {
            data.resize(MIN_ROM_LEN, 0x00)
        }
        Rom {
            data: data.into_boxed_slice(),
        }
    }
}

const MIN_ROM_LEN: usize = 0x8000;

pub struct Rom {
    pub data: Box<[u8]>,
}

pub struct RelocExprBuilder<S>(PhantomData<S>);

impl<S> RelocExprBuilder<S> {
    pub fn new() -> Self {
        RelocExprBuilder(PhantomData)
    }
}

impl<S: Clone + Debug + PartialEq> Span for RelocExprBuilder<S> {
    type Span = S;
}

impl<S: Clone + Debug + PartialEq> ValueBuilder<RelocExpr<S>> for RelocExprBuilder<S> {
    fn location(&mut self, span: S) -> RelocExpr<S> {
        RelocExpr::from_atom(RelocAtom::LocationCounter, span)
    }

    fn number(&mut self, (number, span): (i32, S)) -> RelocExpr<S> {
        RelocExpr::from_atom(RelocAtom::Literal(number), span)
    }

    fn symbol(&mut self, (symbol, span): (String, S)) -> RelocExpr<S> {
        RelocExpr::from_atom(RelocAtom::Symbol(symbol), span)
    }

    fn apply_binary_operator(
        &mut self,
        operator: (BinaryOperator, S),
        left: RelocExpr<S>,
        right: RelocExpr<S>,
    ) -> RelocExpr<S> {
        Expr {
            variant: ExprVariant::Binary(operator.0, Box::new(left), Box::new(right)),
            span: operator.1,
        }
    }
}

impl<S: Clone + Debug + PartialEq> Span for ObjectBuilder<S> {
    type Span = S;
}

impl<S: Clone + Debug + PartialEq> HasValue for ObjectBuilder<S> {
    type Value = RelocExpr<S>;
}

impl<'a, S: Clone + Debug + PartialEq> BuildValue<'a, RelocExpr<S>> for ObjectBuilder<S> {
    type Builder = RelocExprBuilder<S>;

    fn build_value(&'a mut self) -> Self::Builder {
        RelocExprBuilder::new()
    }
}

impl<S: Clone + Debug + PartialEq> Backend<S> for ObjectBuilder<S> {
    type Object = Object<S>;

    fn define_symbol(&mut self, symbol: (impl Into<String>, S), value: Self::Value) {
        self.push(Node::Symbol((symbol.0.into(), symbol.1), value))
    }

    fn emit_item(&mut self, item: Item<RelocExpr<S>>) {
        item.lower().for_each(|data_item| self.push(data_item))
    }

    fn into_object(self) -> Self::Object {
        self.build()
    }

    fn set_origin(&mut self, origin: RelocExpr<S>) {
        self.constrain_origin(origin)
    }
}

pub struct BinarySection {
    origin: usize,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{CompactDiagnostic, Message, TestDiagnosticsListener};
    use crate::instruction::Nullary;
    use std::borrow::Borrow;

    #[test]
    fn empty_object_converted_to_all_zero_rom() {
        let object = BinaryObject {
            sections: Vec::new(),
        };
        let rom = object.into_rom();
        assert_eq!(*rom.data, [0x00u8; MIN_ROM_LEN][..])
    }

    #[test]
    fn chunk_placed_in_rom_starting_at_origin() {
        let byte = 0x42;
        let origin = 0x150;
        let object = BinaryObject {
            sections: vec![BinarySection {
                origin,
                data: vec![byte],
            }],
        };
        let rom = object.into_rom();
        let mut expected = [0x00u8; MIN_ROM_LEN];
        expected[origin] = byte;
        assert_eq!(*rom.data, expected[..])
    }

    #[test]
    fn empty_chunk_does_not_extend_rom() {
        let origin = MIN_ROM_LEN + 1;
        let object = BinaryObject {
            sections: vec![BinarySection {
                origin,
                data: Vec::new(),
            }],
        };
        let rom = object.into_rom();
        assert_eq!(rom.data.len(), MIN_ROM_LEN)
    }

    #[test]
    fn emit_literal_byte_item() {
        emit_items_and_compare([byte_literal(0xff)], [0xff])
    }

    #[test]
    fn emit_two_literal_byte_item() {
        emit_items_and_compare([byte_literal(0x12), byte_literal(0x34)], [0x12, 0x34])
    }

    fn byte_literal(value: i32) -> Item<RelocExpr<()>> {
        Item::Data(value.into(), Width::Byte)
    }

    #[test]
    fn emit_stop() {
        emit_items_and_compare(
            [Item::Instruction(Instruction::Nullary(Nullary::Stop))],
            [0x10, 0x00],
        )
    }

    fn emit_items_and_compare<I, B>(items: I, bytes: B)
    where
        I: Borrow<[Item<RelocExpr<()>>]>,
        B: Borrow<[u8]>,
    {
        let (object, _) = with_object_builder(|builder| {
            for item in items.borrow() {
                builder.emit_item(item.clone())
            }
        });
        assert_eq!(object.sections.last().unwrap().data, bytes.borrow())
    }

    #[test]
    fn emit_diagnostic_when_byte_item_out_of_range() {
        test_diagnostic_for_out_of_range_byte(i8::min_value() as i32 - 1);
        test_diagnostic_for_out_of_range_byte(u8::max_value() as i32 + 1)
    }

    fn test_diagnostic_for_out_of_range_byte(value: i32) {
        let (_, diagnostics) =
            with_object_builder(|builder| builder.emit_item(byte_literal(value)));
        assert_eq!(
            *diagnostics,
            [CompactDiagnostic::new(
                Message::ValueOutOfRange {
                    value,
                    width: Width::Byte,
                },
                ()
            )]
        );
    }

    #[test]
    fn diagnose_unresolved_symbol() {
        let ident = "ident";
        let (_, diagnostics) =
            with_object_builder(|builder| builder.emit_item(symbol_expr_item(ident)));
        assert_eq!(*diagnostics, [unresolved(ident)]);
    }

    #[test]
    fn diagnose_two_unresolved_symbols_in_one_expr() {
        let ident1 = "ident1";
        let ident2 = "ident2";
        let (_, diagnostics) = with_object_builder(|builder| {
            builder.emit_item(Item::Data(
                RelocExpr {
                    variant: ExprVariant::Binary(
                        BinaryOperator::Minus,
                        Box::new(symbol_expr(ident1)),
                        Box::new(symbol_expr(ident2)),
                    ),
                    span: (),
                },
                Width::Word,
            ))
        });
        assert_eq!(*diagnostics, [unresolved(ident1), unresolved(ident2)]);
    }

    #[test]
    fn emit_defined_symbol() {
        let label = "label";
        let (object, diagnostics) = with_object_builder(|builder| {
            builder.define_symbol((label, ()), RelocAtom::LocationCounter.into());
            builder.emit_item(symbol_expr_item(label));
        });
        assert_eq!(*diagnostics, []);
        assert_eq!(object.sections.last().unwrap().data, [0x00, 0x00])
    }

    #[test]
    fn emit_symbol_defined_after_use() {
        let label = "label";
        let (object, diagnostics) = with_object_builder(|builder| {
            builder.emit_item(symbol_expr_item(label));
            builder.define_symbol((label, ()), RelocAtom::LocationCounter.into());
        });
        assert_eq!(*diagnostics, []);
        assert_eq!(object.sections.last().unwrap().data, [0x02, 0x00])
    }

    type TestObjectBuilder = ObjectBuilder<()>;

    fn with_object_builder<F: FnOnce(&mut TestObjectBuilder)>(
        f: F,
    ) -> (BinaryObject, Box<[CompactDiagnostic<()>]>) {
        let mut diagnostics = TestDiagnosticsListener::new();
        let object = {
            let mut builder = ObjectBuilder::new();
            f(&mut builder);
            link(builder.build(), &mut diagnostics)
        };
        let diagnostics = diagnostics.diagnostics.into_inner().into_boxed_slice();
        (object, diagnostics)
    }

    fn symbol_expr_item(symbol: impl Into<String>) -> Item<RelocExpr<()>> {
        Item::Data(symbol_expr(symbol), Width::Word)
    }

    fn symbol_expr(symbol: impl Into<String>) -> RelocExpr<()> {
        RelocExpr {
            variant: ExprVariant::Atom(RelocAtom::Symbol(symbol.into())),
            span: (),
        }
    }

    fn unresolved(symbol: impl Into<String>) -> CompactDiagnostic<()> {
        CompactDiagnostic::new(
            Message::UnresolvedSymbol {
                symbol: symbol.into(),
            },
            (),
        )
    }
}
