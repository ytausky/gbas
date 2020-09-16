use self::builder::{Backend, ObjectBuilder, SymbolSource};
use self::reentrancy::ReentrancyActions;
use self::resolve::*;

use crate::analyze::macros::{MacroId, MacroSource, VecMacroTable};
use crate::analyze::{CodebaseAnalyzer, Literal, StringSource, Tokenizer};
use crate::codebase::{BufId, BufRange, FileCodebase, FileSystem};
use crate::diag::{CompositeDiagnosticsSystem, Diagnostics, OutputForwarder};
use crate::object::SymbolId;
use crate::semantics::keywords::KEYWORDS;
use crate::span::{MacroDefSpans, RcContextFactory, RcSpan, SpanSource};
use crate::syntax::{IdentFactory, IdentSource};
use crate::BuiltinSymbols;

use std::rc::Rc;

pub mod builder;
pub mod reentrancy;
pub mod resolve;

pub(crate) trait Session:
    SpanSource
    + ReentrancyActions
    + Backend<<Self as SpanSource>::Span>
    + Diagnostics<<Self as SpanSource>::Span>
    + StartScope<<Self as IdentSource>::Ident>
    + NameTable<<Self as IdentSource>::Ident>
{
}

impl<T> Session for T where
    Self: SpanSource
        + ReentrancyActions
        + Backend<<Self as SpanSource>::Span>
        + Diagnostics<<Self as SpanSource>::Span>
        + StartScope<<Self as IdentSource>::Ident>
        + NameTable<<Self as IdentSource>::Ident>
{
}

pub(crate) struct SessionImpl<'a, 'b> {
    pub codebase: CodebaseAnalyzer<'b, Tokenizer<&'b FileCodebase<'a, dyn FileSystem>>>,
    pub macros:
        VecMacroTable<Ident<String>, Literal<String>, Rc<MacroDefSpans<RcSpan<BufId, BufRange>>>>,
    names: BiLevelNameTable<BasicNameTable<MacroId, SymbolId>>,
    pub builder: ObjectBuilder<RcSpan<BufId, BufRange>>,
    pub diagnostics: CompositeDiagnosticsSystem<RcContextFactory, OutputForwarder<'a, 'b>>,
}

impl<'a, 'b> SessionImpl<'a, 'b> {
    pub fn new(
        codebase: CodebaseAnalyzer<'b, Tokenizer<&'b FileCodebase<'a, dyn FileSystem>>>,
        diagnostics: CompositeDiagnosticsSystem<RcContextFactory, OutputForwarder<'a, 'b>>,
    ) -> Self {
        let mut session = Self {
            codebase,
            macros: VecMacroTable::new(),
            names: BiLevelNameTable::new(),
            builder: ObjectBuilder::new(),
            diagnostics,
        };
        for (string, name) in session.builder.builtin_symbols() {
            session.names.define_name(
                DefaultIdentFactory.mk_ident(string),
                ResolvedName::Symbol(*name),
            )
        }
        for (ident, keyword) in KEYWORDS {
            session.define_name((*ident).into(), ResolvedName::Keyword(keyword))
        }
        session
    }
}

impl<'a, 'b> SpanSource for SessionImpl<'a, 'b> {
    type Span = RcSpan<BufId, BufRange>;
}

impl<'a, 'b> IdentSource for SessionImpl<'a, 'b> {
    type Ident = Ident<String>;
}

impl<'a, 'b> StringSource for SessionImpl<'a, 'b> {
    type StringRef = String;
}

pub(crate) struct CompositeSession<R, N, B, D> {
    pub reentrancy: R,
    pub names: N,
    pub builder: B,
    pub diagnostics: D,
}

#[cfg(test)]
impl<R, N, B, D> CompositeSession<R, N, B, D>
where
    Self: ReentrancyActions,
    <Self as IdentSource>::Ident: for<'r> From<&'r str>,
    Self: NameTable<<Self as IdentSource>::Ident>,
    Self: Backend<<Self as SpanSource>::Span>,
{
    pub fn from_components(reentrancy: R, names: N, builder: B, diagnostics: D) -> Self {
        let mut session = Self {
            reentrancy,
            names,
            builder,
            diagnostics,
        };
        for (ident, keyword) in KEYWORDS {
            session.define_name((*ident).into(), ResolvedName::Keyword(keyword))
        }
        session
    }
}

impl<R: SpanSource, N, B, D> SpanSource for CompositeSession<R, N, B, D> {
    type Span = R::Span;
}

impl<R: IdentSource, N, B, D> IdentSource for CompositeSession<R, N, B, D> {
    type Ident = R::Ident;
}

impl<R: MacroSource, N, B, D> MacroSource for CompositeSession<R, N, B, D> {
    type MacroId = R::MacroId;
}

impl<R: StringSource, N, B, D> StringSource for CompositeSession<R, N, B, D> {
    type StringRef = R::StringRef;
}

impl<R, N, B: SymbolSource, D> SymbolSource for CompositeSession<R, N, B, D> {
    type SymbolId = B::SymbolId;
}

delegate_diagnostics! {
    {R, N, B, D: Diagnostics<S>, S}, CompositeSession<R, N, B, D>, {diagnostics}, D, S
}

delegate_diagnostics! {
    {'a, 'b},
    SessionImpl<'a, 'b>,
    {diagnostics},
    CompositeDiagnosticsSystem<RcContextFactory, OutputForwarder<'a, 'b>>,
    RcSpan<BufId, BufRange>
}
