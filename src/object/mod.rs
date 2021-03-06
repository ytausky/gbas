use self::var::Var;

use crate::expr::{Atom, ExprOp};
use crate::span::SpanSource;

use std::ops::{Index, IndexMut, Range, RangeInclusive};

pub mod var;

pub struct Object(pub(crate) ObjectData<Metadata, Box<str>>);

pub(crate) struct ObjectData<M: SpanSource, I> {
    pub content: Content<I, M::Span>,
    pub metadata: M,
}

pub struct Content<I, S> {
    pub sections: Vec<Section<S>>,
    pub symbols: Vec<Symbol<I, S>>,
    pub vars: usize,
}

pub struct Section<S> {
    pub constraints: Constraints<S>,
    pub addr: VarId,
    pub size: VarId,
    pub fragments: Vec<Fragment<Expr<S>>>,
}

pub struct Constraints<S> {
    pub addr: Option<Expr<S>>,
}

pub type Expr<S> = crate::expr::Expr<Name, S>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Name {
    Builtin(BuiltinId),
    Symbol(SymbolId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BuiltinId {
    Sizeof,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SymbolId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VarId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub enum Fragment<E> {
    Byte(u8),
    Immediate(E, Width),
    LdInlineAddr(u8, E),
    Embedded(u8, E),
    Reloc(VarId),
    Reserved(E),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    Byte,
    Word,
}

#[derive(Debug, PartialEq)]
pub enum Symbol<I, S> {
    Exported { ident: I, def: SymbolDefRecord<S> },
    Local { def: SymbolDefRecord<S> },
    Unknown { ident: I },
}

#[derive(Debug, PartialEq)]
pub struct SymbolDefRecord<S> {
    pub def_ident_span: S,
    pub meaning: SymbolMeaning<S>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SymbolMeaning<S> {
    Closure(Closure<S>),
    Section(SectionId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Closure<S> {
    pub expr: Expr<S>,
    pub location: VarId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionId(pub usize);

pub struct VarTable(pub Vec<Var>);

#[derive(Default)]
pub struct Metadata {
    pub source_files: SourceFileTable,
    pub span_data: SpanData,
}

pub type SourceFileTable = Box<[Box<str>]>;

pub struct SpanData {
    pub(crate) source_file_inclusions: Vec<FileInclusionMetadata<Span>>,
    pub(crate) macro_defs: Vec<MacroDefMetadata<Span>>,
    pub(crate) macro_expansions: Vec<MacroExpansionMetadata<Span>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceFileId(pub usize);

#[derive(Clone, Copy, Debug)]
pub struct MacroDefId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceFileInclusionId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroExpansionId(pub usize);

#[derive(Debug)]
pub struct FileInclusionMetadata<S> {
    pub file: SourceFileId,
    pub from: Option<S>,
}

#[derive(Debug)]
pub struct MacroDefMetadata<S> {
    pub name_span: S,
    pub param_spans: Box<[S]>,
    pub body_spans: Box<[S]>,
}

#[derive(Debug)]
pub struct MacroExpansionMetadata<S> {
    pub def: MacroDefId,
    pub name_span: S,
    pub arg_spans: Box<[Box<[S]>]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Span {
    SourceFile {
        inclusion_metadata: SourceFileInclusionId,
        range: SourceFileRange,
    },
    MacroExpansion {
        metadata: MacroExpansionId,
        range: RangeInclusive<MacroExpansionPos>,
    },
}

pub type SourceFileRange = Range<usize>;

#[derive(Clone, Debug, PartialEq)]
pub struct MacroExpansionPos {
    pub token: usize,
    pub param_expansion: Option<ParamExpansionPos>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamExpansionPos {
    pub param: usize,
    pub arg_token: usize,
}

impl SpanSource for Metadata {
    type Span = Span;
}

impl<I, S> Content<I, S> {
    pub fn new() -> Content<I, S> {
        Content {
            sections: Vec::new(),
            symbols: Vec::new(),
            vars: 0,
        }
    }

    pub fn sections(&self) -> impl Iterator<Item = &Section<S>> {
        self.sections.iter()
    }
}

pub(super) struct LinkageContext<C, V> {
    pub content: C,
    pub vars: V,
    pub location: Var,
}

impl VarTable {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn alloc(&mut self) -> VarId {
        let id = VarId(self.0.len());
        self.0.push(Default::default());
        id
    }
}

impl Index<VarId> for VarTable {
    type Output = Var;

    fn index(&self, VarId(id): VarId) -> &Self::Output {
        &self.0[id as usize]
    }
}

impl IndexMut<VarId> for VarTable {
    fn index_mut(&mut self, VarId(id): VarId) -> &mut Self::Output {
        &mut self.0[id as usize]
    }
}

impl From<Name> for Atom<Name> {
    fn from(id: Name) -> Self {
        Atom::Name(id)
    }
}

#[cfg(test)]
impl From<SymbolId> for Atom<Name> {
    fn from(id: SymbolId) -> Self {
        Atom::Name(id.into())
    }
}

impl From<Name> for ExprOp<Name> {
    fn from(id: Name) -> Self {
        Atom::from(id).into()
    }
}

#[cfg(test)]
impl From<BuiltinId> for ExprOp<Name> {
    fn from(builtin: BuiltinId) -> Self {
        Atom::from(Name::from(builtin)).into()
    }
}

#[cfg(test)]
impl From<SymbolId> for ExprOp<Name> {
    fn from(id: SymbolId) -> Self {
        Atom::from(id).into()
    }
}

impl From<BuiltinId> for Name {
    fn from(builtin: BuiltinId) -> Self {
        Name::Builtin(builtin)
    }
}

impl From<SymbolId> for Name {
    fn from(id: SymbolId) -> Self {
        Name::Symbol(id)
    }
}
