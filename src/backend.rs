use crate::expr::{BinaryOperator, Expr, ExprVariant};
use crate::frontend::Ident;
use crate::instruction::Instruction;
use crate::program::NameId;
use crate::span::Source;
#[cfg(test)]
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(test)]
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    Byte,
    Word,
}

pub trait NameTable<I> {
    type MacroEntry;

    fn get(&self, ident: &I) -> Option<&Name<Self::MacroEntry>>;
    fn insert(&mut self, ident: I, entry: Name<Self::MacroEntry>);
}

pub struct HashMapNameTable<M> {
    table: HashMap<String, Name<M>>,
}

pub enum Name<M> {
    Macro(M),
    Symbol(NameId),
}

impl<M> HashMapNameTable<M> {
    pub fn new() -> Self {
        HashMapNameTable {
            table: HashMap::new(),
        }
    }
}

impl<M> NameTable<Ident<String>> for HashMapNameTable<M> {
    type MacroEntry = M;

    fn get(&self, ident: &Ident<String>) -> Option<&Name<Self::MacroEntry>> {
        self.table.get(&ident.name)
    }

    fn insert(&mut self, ident: Ident<String>, entry: Name<Self::MacroEntry>) {
        self.table.insert(ident.name, entry);
    }
}

pub struct LocationCounter;

pub trait HasValue<S: Clone> {
    type Value: Source<Span = S>;
}

pub trait BuildValue<'a, I, N, S: Clone>
where
    Self: HasValue<S>,
{
    type Builder: ValueBuilder<I, S, Value = Self::Value>;
    fn build_value(&'a mut self, names: &'a mut N) -> Self::Builder;
}

pub trait ValueBuilder<I, S: Clone>
where
    Self: ToValue<LocationCounter, S>,
    Self: ToValue<i32, S>,
    Self: ToValue<I, S>,
    Self: ApplyBinaryOperator<S>,
{
}

impl<T, I, S: Clone> ValueBuilder<I, S> for T
where
    T: ToValue<LocationCounter, S>,
    T: ToValue<i32, S>,
    T: ToValue<I, S>,
    T: ApplyBinaryOperator<S>,
{
}

pub trait ToValue<T, S: Clone>
where
    Self: HasValue<S>,
{
    fn to_value(&mut self, atom: (T, S)) -> Self::Value;
}

pub trait ApplyBinaryOperator<S: Clone>
where
    Self: HasValue<S>,
{
    fn apply_binary_operator(
        &mut self,
        operator: (BinaryOperator, S),
        left: Self::Value,
        right: Self::Value,
    ) -> Self::Value;
}

pub trait PartialBackend<S>
where
    S: Clone,
    Self: HasValue<S>,
{
    fn emit_item(&mut self, item: Item<Self::Value>);
    fn set_origin(&mut self, origin: Self::Value);
}

pub trait Backend<I, S, N>
where
    S: Clone,
    Self: PartialBackend<S>,
    for<'a> Self: BuildValue<'a, I, N, S>,
{
    fn define_symbol(&mut self, symbol: (I, S), value: Self::Value, names: &mut N);
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item<V: Source> {
    Data(V, Width),
    Instruction(Instruction<V>),
}

pub type RelocExpr<I, S> = Expr<RelocAtom<I>, Empty, BinaryOperator, S>;

#[derive(Clone, Debug, PartialEq)]
pub enum Empty {}

#[derive(Clone, Debug, PartialEq)]
pub enum RelocAtom<I> {
    Literal(i32),
    LocationCounter,
    Symbol(I),
}

impl<I, S> From<i32> for ExprVariant<RelocAtom<I>, Empty, BinaryOperator, S> {
    fn from(n: i32) -> Self {
        ExprVariant::Atom(RelocAtom::Literal(n))
    }
}

#[cfg(test)]
impl<I, T: Into<ExprVariant<RelocAtom<I>, Empty, BinaryOperator, ()>>> From<T>
    for RelocExpr<I, ()>
{
    fn from(variant: T) -> Self {
        Expr {
            variant: variant.into(),
            span: (),
        }
    }
}

pub struct BinaryObject {
    pub sections: Vec<BinarySection>,
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

pub struct RelocExprBuilder<'a, T, N>(pub T, pub &'a mut N);

#[cfg(test)]
pub type IndependentValueBuilder<'a, S, N> = RelocExprBuilder<'a, PhantomData<S>, N>;

#[cfg(test)]
impl<'a, S, N> IndependentValueBuilder<'a, S, N> {
    pub fn new(names: &'a mut N) -> Self {
        RelocExprBuilder(PhantomData, names)
    }
}

#[cfg(test)]
impl<'a, S: Clone, N> HasValue<S> for IndependentValueBuilder<'a, S, N> {
    type Value = RelocExpr<Ident<String>, S>;
}

impl<'a, I, T, S: Clone, N> ToValue<LocationCounter, S> for RelocExprBuilder<'a, T, N>
where
    Self: HasValue<S, Value = RelocExpr<I, S>>,
{
    fn to_value(&mut self, (_, span): (LocationCounter, S)) -> Self::Value {
        RelocExpr::from_atom(RelocAtom::LocationCounter, span)
    }
}

impl<'a, I, T, S: Clone, N> ToValue<i32, S> for RelocExprBuilder<'a, T, N>
where
    Self: HasValue<S, Value = RelocExpr<I, S>>,
{
    fn to_value(&mut self, (number, span): (i32, S)) -> Self::Value {
        RelocExpr::from_atom(RelocAtom::Literal(number), span)
    }
}

#[cfg(test)]
impl<'a, S: Clone, N> ToValue<Ident<String>, S> for IndependentValueBuilder<'a, S, N> {
    fn to_value(&mut self, (name, span): (Ident<String>, S)) -> Self::Value {
        RelocExpr::from_atom(RelocAtom::Symbol(name), span)
    }
}

impl<'a, I, T, S: Clone, N> ApplyBinaryOperator<S> for RelocExprBuilder<'a, T, N>
where
    Self: HasValue<S, Value = RelocExpr<I, S>>,
{
    fn apply_binary_operator(
        &mut self,
        operator: (BinaryOperator, S),
        left: Self::Value,
        right: Self::Value,
    ) -> Self::Value {
        Expr {
            variant: ExprVariant::Binary(operator.0, Box::new(left), Box::new(right)),
            span: operator.1,
        }
    }
}

pub struct BinarySection {
    pub origin: usize,
    pub data: Vec<u8>,
}

#[cfg(test)]
pub struct MockBackend<'a, T> {
    pub log: &'a RefCell<Vec<T>>,
}

#[cfg(test)]
#[derive(Debug, PartialEq)]
pub enum Event<V: Source> {
    EmitItem(Item<V>),
    SetOrigin(V),
    DefineSymbol((Ident<String>, V::Span), V),
}

#[cfg(test)]
impl<'a, T> MockBackend<'a, T> {
    pub fn new(log: &'a RefCell<Vec<T>>) -> Self {
        MockBackend { log }
    }
}

#[cfg(test)]
impl<'a, T, S, N> Backend<Ident<String>, S, N> for MockBackend<'a, T>
where
    T: From<Event<RelocExpr<Ident<String>, S>>>,
    S: Clone,
    N: 'static,
{
    fn define_symbol(&mut self, symbol: (Ident<String>, S), value: Self::Value, _: &mut N) {
        self.log
            .borrow_mut()
            .push(Event::DefineSymbol(symbol, value).into())
    }
}

#[cfg(test)]
impl<'a, 'b, T, N, S> BuildValue<'b, Ident<String>, N, S> for MockBackend<'a, T>
where
    T: From<Event<RelocExpr<Ident<String>, S>>>,
    N: 'b,
    S: Clone,
{
    type Builder = IndependentValueBuilder<'b, S, N>;

    fn build_value(&'b mut self, names: &'b mut N) -> Self::Builder {
        IndependentValueBuilder::new(names)
    }
}

#[cfg(test)]
impl<'a, T, S> HasValue<S> for MockBackend<'a, T>
where
    T: From<Event<RelocExpr<Ident<String>, S>>>,
    S: Clone,
{
    type Value = RelocExpr<Ident<String>, S>;
}

#[cfg(test)]
impl<'a, T, S> PartialBackend<S> for MockBackend<'a, T>
where
    T: From<Event<RelocExpr<Ident<String>, S>>>,
    S: Clone,
{
    fn emit_item(&mut self, item: Item<Self::Value>) {
        self.log.borrow_mut().push(Event::EmitItem(item).into())
    }

    fn set_origin(&mut self, origin: Self::Value) {
        self.log.borrow_mut().push(Event::SetOrigin(origin).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
