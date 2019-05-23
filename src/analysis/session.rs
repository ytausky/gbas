pub use super::backend::ValueBuilder;

use super::backend::*;
use super::macros::{DefineMacro, Expand, MacroEntry};
use super::resolve::{Ident, Name, NameTable, StartScope};
use super::semantics::Analyze;
use super::{Lex, SemanticToken, StringSource};

use crate::codebase::CodebaseError;
use crate::diag::span::SpanSource;
use crate::diag::*;
use crate::model::Item;

#[cfg(test)]
pub(crate) use self::mock::*;

pub(super) trait Session
where
    Self: SpanSource + StringSource,
    Self: BasicSession<<Self as StringSource>::StringRef, <Self as SpanSource>::Span>,
{
    fn analyze_file(self, path: Self::StringRef) -> (Result<(), CodebaseError>, Self);

    fn define_macro(
        &mut self,
        name: (Ident<Self::StringRef>, Self::Span),
        params: Params<Self::StringRef, Self::Span>,
        body: (Vec<SemanticToken<Self::StringRef>>, Vec<Self::Span>),
    );
    fn call_macro(
        self,
        name: (Ident<Self::StringRef>, Self::Span),
        args: MacroArgs<Self::StringRef, Self::Span>,
    ) -> Self;
}

pub(super) trait BasicSession<R, S: Clone>
where
    Self: Sized,
    Self: PartialBackend<S>,
    Self: StartSection<Ident<R>, S>,
    Self: Diagnostics<S>,
{
    type FnBuilder: ValueBuilder<Ident<R>, S> + FinishFnDef<Return = Self> + Diagnostics<S>;
    type GeneralBuilder: ValueBuilder<Ident<R>, S>
        + Finish<S, Parent = Self, Value = Self::Value>
        + Diagnostics<S>;

    fn build_value(self) -> Self::GeneralBuilder;
    fn define_symbol(self, name: Ident<R>, span: S) -> Self::FnBuilder;
}

pub(super) type MacroArgs<I, S> = Vec<Vec<(SemanticToken<I>, S)>>;
pub(super) type Params<R, S> = (Vec<Ident<R>>, Vec<S>);

pub(super) struct CompositeSession<'a, 'b, C, A, B, N, D> {
    codebase: &'b mut C,
    analyzer: &'a mut A,
    backend: B,
    names: &'b mut N,
    diagnostics: &'b mut D,
}

impl<'a, 'b, C, A, B, N, D> CompositeSession<'a, 'b, C, A, B, N, D> {
    pub fn new(
        codebase: &'b mut C,
        analyzer: &'a mut A,
        backend: B,
        names: &'b mut N,
        diagnostics: &'b mut D,
    ) -> Self {
        CompositeSession {
            codebase,
            analyzer,
            backend,
            names,
            diagnostics,
        }
    }
}

impl<'a, 'b, C, A, B, N, D> CompositeSession<'a, 'b, C, A, B, N, D> {
    fn look_up_symbol<R, S>(&mut self, ident: Ident<R>, span: &S) -> B::Name
    where
        B: AllocName<S>,
        N: NameTable<Ident<R>, BackendEntry = B::Name>,
        S: Clone,
    {
        look_up_symbol(&mut self.backend, self.names, ident, span)
    }
}

fn look_up_symbol<B, N, R, S>(backend: &mut B, names: &mut N, ident: Ident<R>, span: &S) -> B::Name
where
    B: AllocName<S>,
    N: NameTable<Ident<R>, BackendEntry = B::Name>,
    S: Clone,
{
    match names.get(&ident) {
        Some(Name::Backend(id)) => id.clone(),
        Some(Name::Macro(_)) => unimplemented!(),
        None => {
            let id = backend.alloc_name(span.clone());
            names.insert(ident, Name::Backend(id.clone()));
            id
        }
    }
}

pub struct PartialSession<'b, C: 'b, B, N: 'b, D: 'b> {
    pub codebase: &'b mut C,
    pub backend: B,
    pub names: &'b mut N,
    pub diagnostics: &'b mut D,
}

macro_rules! partial {
    ($session:expr) => {
        PartialSession {
            codebase: $session.codebase,
            backend: $session.backend,
            names: $session.names,
            diagnostics: $session.diagnostics,
        }
    };
}

impl<'a, 'b, C, A, B, N, D> From<CompositeSession<'a, 'b, C, A, B, N, D>>
    for PartialSession<'b, C, B, N, D>
{
    fn from(session: CompositeSession<'a, 'b, C, A, B, N, D>) -> Self {
        partial!(session)
    }
}

impl<'a, 'b, F, A, B, N, D> SpanSource for CompositeSession<'a, 'b, F, A, B, N, D>
where
    D: SpanSource,
{
    type Span = D::Span;
}

impl<'a, 'b, C: StringSource, A, B, N, D> StringSource for CompositeSession<'a, 'b, C, A, B, N, D> {
    type StringRef = C::StringRef;
}

impl<'a, 'b, C, A, B, N, D, S> PartialBackend<S> for CompositeSession<'a, 'b, C, A, B, N, D>
where
    B: Backend<S>,
    S: Clone,
{
    type Value = B::Value;

    fn emit_item(&mut self, item: Item<Self::Value>) {
        self.backend.emit_item(item)
    }

    fn reserve(&mut self, bytes: Self::Value) {
        self.backend.reserve(bytes)
    }

    fn set_origin(&mut self, origin: Self::Value) {
        self.backend.set_origin(origin)
    }
}

impl<'a, 'b, C, A, B, N, D, R, S> PushOp<Ident<R>, S>
    for RelocContext<CompositeSession<'a, 'b, C, A, (), N, D>, B>
where
    B: AllocName<S> + PushOp<<B as AllocName<S>>::Name, S>,
    N: NameTable<Ident<R>, BackendEntry = B::Name>,
    S: Clone,
{
    fn push_op(&mut self, ident: Ident<R>, span: S) {
        let id = look_up_symbol(&mut self.builder, self.parent.names, ident, &span);
        self.builder.push_op(id, span)
    }
}

impl<'a, 'b, C, A, B, N, D, S> Finish<S>
    for RelocContext<CompositeSession<'a, 'b, C, A, (), N, D>, B>
where
    B: Finish<S>,
    S: Clone,
{
    type Parent = CompositeSession<'a, 'b, C, A, B::Parent, N, D>;
    type Value = B::Value;

    fn finish(self) -> (Self::Parent, Self::Value) {
        let (backend, value) = self.builder.finish();
        let parent = CompositeSession {
            codebase: self.parent.codebase,
            analyzer: self.parent.analyzer,
            backend,
            names: self.parent.names,
            diagnostics: self.parent.diagnostics,
        };
        (parent, value)
    }
}

impl<'a, 'b, C, A, B, N, D> FinishFnDef
    for RelocContext<CompositeSession<'a, 'b, C, A, (), N, D>, B>
where
    B: FinishFnDef,
{
    type Return = CompositeSession<'a, 'b, C, A, B::Return, N, D>;

    fn finish_fn_def(self) -> Self::Return {
        CompositeSession {
            codebase: self.parent.codebase,
            analyzer: self.parent.analyzer,
            backend: self.builder.finish_fn_def(),
            names: self.parent.names,
            diagnostics: self.parent.diagnostics,
        }
    }
}

delegate_diagnostics! {
    {P: Diagnostics<S>, B, S}, RelocContext<P, B>, {parent}, P, S
}

impl<'a, 'b, C, A, B, N, D> Session for CompositeSession<'a, 'b, C, A, B, N, D>
where
    C: Lex<D>,
    A: Analyze<C::StringRef, D>,
    B: Backend<D::Span>,
    N: NameTable<
            Ident<C::StringRef>,
            BackendEntry = B::Name,
            MacroEntry = MacroEntry<C::StringRef, D>,
        > + StartScope<Ident<C::StringRef>>,
    D: DiagnosticsSystem,
{
    fn analyze_file(mut self, path: Self::StringRef) -> (Result<(), CodebaseError>, Self) {
        let tokens = match self.codebase.lex_file(path, self.diagnostics) {
            Ok(tokens) => tokens,
            Err(error) => return (Err(error), self),
        };
        let PartialSession { backend, .. } =
            self.analyzer.analyze_token_seq(tokens, partial!(self));
        self.backend = backend;
        (Ok(()), self)
    }

    fn define_macro(
        &mut self,
        name: (Ident<Self::StringRef>, Self::Span),
        params: (Vec<Ident<Self::StringRef>>, Vec<Self::Span>),
        body: (Vec<SemanticToken<Self::StringRef>>, Vec<Self::Span>),
    ) {
        self.names
            .define_macro(name, params, body, self.diagnostics)
    }

    fn call_macro(
        mut self,
        name: (Ident<Self::StringRef>, Self::Span),
        args: MacroArgs<Self::StringRef, Self::Span>,
    ) -> Self {
        let expansion = match self.names.get(&name.0) {
            Some(Name::Macro(entry)) => Some(entry.expand(name.1, args, self.diagnostics)),
            Some(_) => unimplemented!(),
            None => {
                let stripped = self.diagnostics.strip_span(&name.1);
                self.diagnostics
                    .emit_diag(Message::UndefinedMacro { name: stripped }.at(name.1));
                None
            }
        };
        if let Some(expansion) = expansion {
            let PartialSession { backend, .. } = self
                .analyzer
                .analyze_token_seq(expansion.map(|(t, s)| (Ok(t), s)), partial!(self));
            self.backend = backend
        }
        self
    }
}

impl<'a, 'b, C, A, B, N, D, R, S> BasicSession<R, S> for CompositeSession<'a, 'b, C, A, B, N, D>
where
    B: Backend<S>,
    N: NameTable<Ident<R>, BackendEntry = B::Name> + StartScope<Ident<R>>,
    D: Diagnostics<S>,
    S: Clone,
{
    type FnBuilder = RelocContext<CompositeSession<'a, 'b, C, A, (), N, D>, B::SymbolBuilder>;
    type GeneralBuilder =
        RelocContext<CompositeSession<'a, 'b, C, A, (), N, D>, B::ImmediateBuilder>;

    fn build_value(self) -> Self::GeneralBuilder {
        RelocContext {
            parent: CompositeSession {
                codebase: self.codebase,
                analyzer: self.analyzer,
                backend: (),
                names: self.names,
                diagnostics: self.diagnostics,
            },
            builder: self.backend.build_immediate(),
        }
    }

    fn define_symbol(mut self, name: Ident<R>, span: S) -> Self::FnBuilder {
        self.names.start_scope(&name);
        let id = self.look_up_symbol(name, &span);
        let session = CompositeSession {
            codebase: self.codebase,
            analyzer: self.analyzer,
            backend: (),
            names: self.names,
            diagnostics: self.diagnostics,
        };
        RelocContext {
            parent: session,
            builder: self.backend.define_fn(id, span),
        }
    }
}

delegate_diagnostics! {
    {'a, 'b, F, A, B, N, D: Diagnostics<S>, S},
    CompositeSession<'a, 'b, F, A, B, N, D>,
    {diagnostics},
    D,
    S
}

impl<'a, 'b, C, A, B, N, D, R, S> StartSection<Ident<R>, S>
    for CompositeSession<'a, 'b, C, A, B, N, D>
where
    B: Backend<S>,
    N: NameTable<Ident<R>, BackendEntry = B::Name>,
    S: Clone,
{
    fn start_section(&mut self, (ident, span): (Ident<R>, S)) {
        let name = self.look_up_symbol(ident, &span);
        self.backend.start_section((name, span))
    }
}

#[cfg(test)]
mod mock {
    use super::*;

    use crate::analysis::backend::{BackendEvent, MockSymbolBuilder};
    use crate::diag::{DiagnosticsEvent, MockDiagnostics};

    use std::cell::RefCell;
    use std::marker::PhantomData;

    type Expr<S> = crate::model::Expr<LocationCounter, Ident<String>, S>;

    #[derive(Debug, PartialEq)]
    pub(crate) enum SessionEvent<S> {
        AnalyzeFile(String),
        DefineMacro(
            Ident<String>,
            Vec<Ident<String>>,
            Vec<SemanticToken<String>>,
        ),
        InvokeMacro(Ident<String>, Vec<Vec<SemanticToken<String>>>),
        DefineSymbol((Ident<String>, S), Expr<S>),
    }

    pub(crate) struct MockSession<'a, T, S> {
        log: &'a RefCell<Vec<T>>,
        error: Option<CodebaseError>,
        diagnostics: MockDiagnostics<'a, T>,
        _span: PhantomData<S>,
    }

    impl<'a, T, S> MockSession<'a, T, S> {
        pub fn new(log: &'a RefCell<Vec<T>>) -> Self {
            Self {
                log,
                error: None,
                diagnostics: MockDiagnostics::new(log),
                _span: PhantomData,
            }
        }

        pub fn fail(&mut self, error: CodebaseError) {
            self.error = Some(error)
        }
    }

    delegate_diagnostics! {
        {'a, T: From<DiagnosticsEvent<S>>, S: Merge},
        MockSession<'a, T, S>,
        {diagnostics},
        MockDiagnostics<'a, T>,
        S
    }

    impl<'a, T, S: Clone + Merge> SpanSource for MockSession<'a, T, S> {
        type Span = S;
    }

    impl<'a, T, S> StringSource for MockSession<'a, T, S> {
        type StringRef = String;
    }

    impl<'a, T, S> Session for MockSession<'a, T, S>
    where
        T: From<SessionEvent<S>>,
        T: From<BackendEvent<Expr<S>>>,
        T: From<DiagnosticsEvent<S>>,
        S: Clone + Merge,
    {
        fn analyze_file(mut self, path: String) -> (Result<(), CodebaseError>, Self) {
            self.log
                .borrow_mut()
                .push(SessionEvent::AnalyzeFile(path).into());
            (self.error.take().map_or(Ok(()), Err), self)
        }

        fn define_macro(
            &mut self,
            name: (Ident<Self::StringRef>, Self::Span),
            params: (Vec<Ident<Self::StringRef>>, Vec<Self::Span>),
            body: (Vec<SemanticToken<Self::StringRef>>, Vec<Self::Span>),
        ) {
            self.log
                .borrow_mut()
                .push(SessionEvent::DefineMacro(name.0, params.0, body.0).into())
        }

        fn call_macro(
            self,
            name: (Ident<Self::StringRef>, Self::Span),
            args: MacroArgs<Self::StringRef, Self::Span>,
        ) -> Self {
            self.log.borrow_mut().push(
                SessionEvent::InvokeMacro(
                    name.0,
                    args.into_iter()
                        .map(|arg| arg.into_iter().map(|(token, _)| token).collect())
                        .collect(),
                )
                .into(),
            );
            self
        }
    }

    impl<'a, T, S> BasicSession<String, S> for MockSession<'a, T, S>
    where
        T: From<SessionEvent<S>>,
        T: From<BackendEvent<Expr<S>>>,
        T: From<DiagnosticsEvent<S>>,
        S: Clone + Merge,
    {
        type FnBuilder = MockSymbolBuilder<Self, Ident<String>, S>;
        type GeneralBuilder = RelocContext<Self, Expr<S>>;

        fn build_value(self) -> Self::GeneralBuilder {
            RelocContext::new(self)
        }

        fn define_symbol(self, name: Ident<String>, span: S) -> Self::FnBuilder {
            MockSymbolBuilder {
                parent: self,
                name: (name, span),
                expr: Default::default(),
            }
        }
    }

    impl<'a, T, S: Clone> Finish<S> for RelocContext<MockSession<'a, T, S>, Expr<S>> {
        type Parent = MockSession<'a, T, S>;
        type Value = Expr<S>;

        fn finish(self) -> (Self::Parent, Self::Value) {
            (self.parent, self.builder)
        }
    }

    impl<'a, T, S> FinishFnDef for MockSymbolBuilder<MockSession<'a, T, S>, Ident<String>, S>
    where
        T: From<SessionEvent<S>>,
    {
        type Return = MockSession<'a, T, S>;

        fn finish_fn_def(self) -> Self::Return {
            let parent = self.parent;
            parent
                .log
                .borrow_mut()
                .push(SessionEvent::DefineSymbol(self.name, self.expr).into());
            parent
        }
    }

    delegate_diagnostics! {
        {'a, T: From<DiagnosticsEvent<S>>, S: Merge},
        MockSymbolBuilder<MockSession<'a, T, S>, Ident<String>, S>,
        {parent.diagnostics},
        MockDiagnostics<'a, T>,
        S
    }

    impl<'a, T, S> PushOp<Ident<String>, S> for RelocContext<MockSession<'a, T, S>, Expr<S>>
    where
        T: From<DiagnosticsEvent<S>>,
        S: Clone,
    {
        fn push_op(&mut self, ident: Ident<String>, span: S) {
            use crate::model::{Atom, ExprItem};
            self.builder.0.push(ExprItem {
                op: Atom::Name(ident).into(),
                op_span: span.clone(),
                expr_span: span,
            })
        }
    }

    impl<'a, T, S> PushOp<Ident<String>, S> for RelocContext<MockDiagnostics<'a, T>, Expr<S>>
    where
        T: From<DiagnosticsEvent<S>>,
        S: Clone,
    {
        fn push_op(&mut self, ident: Ident<String>, span: S) {
            use crate::model::{Atom, ExprItem};
            self.builder.0.push(ExprItem {
                op: Atom::Name(ident).into(),
                op_span: span.clone(),
                expr_span: span,
            })
        }
    }

    impl<'a, T, S> PartialBackend<S> for MockSession<'a, T, S>
    where
        T: From<BackendEvent<Expr<S>>>,
        S: Clone + Merge,
    {
        type Value = Expr<S>;

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

    impl<'a, T, S> StartSection<Ident<String>, S> for MockSession<'a, T, S>
    where
        T: From<BackendEvent<Expr<S>>>,
        S: Clone + Merge,
    {
        fn start_section(&mut self, name: (Ident<String>, S)) {
            self.log
                .borrow_mut()
                .push(BackendEvent::StartSection((0, name.1)).into())
        }
    }

    pub(crate) type MockBuilder<'a, T, S> = RelocContext<MockDiagnostics<'a, T>, Expr<S>>;

    impl<'a, T, S> MockBuilder<'a, T, S> {
        pub fn with_log(log: &'a RefCell<Vec<T>>) -> Self {
            Self {
                parent: MockDiagnostics::new(log),
                builder: Default::default(),
            }
        }
    }

    impl<'a, T, S: Clone> Finish<S> for MockBuilder<'a, T, S> {
        type Parent = MockDiagnostics<'a, T>;
        type Value = Expr<S>;

        fn finish(self) -> (Self::Parent, Self::Value) {
            (self.parent, self.builder)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::analysis::backend::BackendEvent;
    use crate::analysis::resolve::{BasicNameTable, NameTableEvent};
    use crate::analysis::semantics::AnalyzerEvent;
    use crate::analysis::syntax::{Command, Directive, Mnemonic, Token};
    use crate::analysis::{Literal, MockCodebase};
    use crate::diag::DiagnosticsEvent;
    use crate::model::{Atom, BinOp, Instruction, Nullary, Width};

    use std::cell::RefCell;
    use std::iter;

    type Expr<S> = crate::model::Expr<LocationCounter, usize, S>;

    #[test]
    fn emit_instruction_item() {
        let item = Item::Instruction(Instruction::Nullary(Nullary::Nop));
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::<()>::new(&log);
        let mut session = fixture.session();
        session.emit_item(item.clone());
        assert_eq!(log.into_inner(), [BackendEvent::EmitItem(item).into()]);
    }

    #[test]
    fn define_label() {
        let label = "label";
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let session = fixture.session();
        let mut builder = session.define_symbol(label.into(), ());
        builder.push_op(LocationCounter, ());
        builder.finish_fn_def();
        assert_eq!(
            log.into_inner(),
            [
                NameTableEvent::StartScope(label.into()).into(),
                BackendEvent::DefineSymbol((0, ()), LocationCounter.into()).into()
            ]
        );
    }

    #[test]
    fn start_section() {
        let name: Ident<_> = "my_section".into();
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let mut session = fixture.session();
        session.start_section((name.clone(), ()));
        assert_eq!(
            log.into_inner(),
            [BackendEvent::StartSection((0, ())).into()]
        )
    }

    #[test]
    fn look_up_section_name_after_definition() {
        let ident: Ident<_> = "my_section".into();
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let mut session = fixture.session();
        session.start_section((ident.clone(), ()));
        let mut builder = session.build_value();
        builder.push_op(ident, ());
        let (s, value) = Finish::finish(builder);
        let item = Item::Data(value, Width::Word);
        session = s;
        session.emit_item(item);
        assert_eq!(
            log.into_inner(),
            [
                BackendEvent::StartSection((0, ())).into(),
                BackendEvent::EmitItem(Item::Data(Atom::Name(0).into(), Width::Word)).into()
            ]
        )
    }

    #[test]
    fn include_source_file() {
        let path = "my_file.s";
        let tokens = vec![(Ok(Token::Command(Command::Mnemonic(Mnemonic::Nop))), ())];
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        fixture.codebase.set_file(path, tokens.clone());
        let session = fixture.session();
        session.analyze_file(path.into()).0.unwrap();
        assert_eq!(
            log.into_inner(),
            [AnalyzerEvent::AnalyzeTokenSeq(tokens).into()]
        );
    }

    #[test]
    fn define_and_call_macro() {
        let name = "my_macro";
        let tokens = vec![Token::Command(Command::Mnemonic(Mnemonic::Nop))];
        let spans: Vec<_> = iter::repeat(()).take(tokens.len()).collect();
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let mut session = fixture.session();
        session.define_macro(
            (name.into(), ()),
            (vec![], vec![]),
            (tokens.clone(), spans.clone()),
        );
        session.call_macro((name.into(), ()), vec![]);
        assert_eq!(
            log.into_inner(),
            [AnalyzerEvent::AnalyzeTokenSeq(
                tokens.into_iter().map(|token| (Ok(token), ())).collect()
            )
            .into()]
        );
    }

    #[test]
    fn define_and_call_macro_with_param() {
        let db = Token::Command(Command::Directive(Directive::Db));
        let arg = Token::Literal(Literal::Number(0x42));
        let literal0 = Token::Literal(Literal::Number(0));
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let mut session = fixture.session();
        let name = "my_db";
        let param = "x";
        session.define_macro(
            (name.into(), ()),
            (vec![param.into()], vec![()]),
            (
                vec![db.clone(), Token::Ident(param.into()), literal0.clone()],
                vec![(), (), ()],
            ),
        );
        session.call_macro((name.into(), ()), vec![vec![(arg.clone(), ())]]);
        assert_eq!(
            log.into_inner(),
            [AnalyzerEvent::AnalyzeTokenSeq(
                vec![db, arg, literal0]
                    .into_iter()
                    .map(|token| (Ok(token), ()))
                    .collect()
            )
            .into()]
        );
    }

    #[test]
    fn define_and_call_macro_with_label() {
        let nop = Token::Command(Command::Mnemonic(Mnemonic::Nop));
        let label = "label";
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let mut session = fixture.session();
        let name = "my_macro";
        let param = "x";
        session.define_macro(
            (name.into(), ()),
            (vec![param.into()], vec![()]),
            (vec![Token::Label(param.into()), nop.clone()], vec![(), ()]),
        );
        session.call_macro(
            (name.into(), ()),
            vec![vec![(Token::Ident(label.into()), ())]],
        );
        assert_eq!(
            log.into_inner(),
            [AnalyzerEvent::AnalyzeTokenSeq(
                vec![Token::Label(label.into()), nop]
                    .into_iter()
                    .map(|token| (Ok(token), ()))
                    .collect()
            )
            .into()]
        );
    }

    #[test]
    fn reserve_bytes() {
        let bytes = 10;
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let mut session = fixture.session();
        session.reserve(bytes.into());
        assert_eq!(
            log.into_inner(),
            [BackendEvent::Reserve(bytes.into()).into()]
        )
    }

    #[test]
    fn diagnose_undefined_macro() {
        let name = "my_macro";
        let span = name;
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::<MockSpan<_>>::new(&log);
        let session = fixture.session();
        session.call_macro((name.into(), span.into()), vec![]);
        assert_eq!(
            log.into_inner(),
            [DiagnosticsEvent::EmitDiag(
                Message::UndefinedMacro { name: span.into() }
                    .at(span.into())
                    .into()
            )
            .into()]
        );
    }

    impl Default for MockSpan<&'static str> {
        fn default() -> Self {
            unreachable!()
        }
    }

    #[test]
    fn build_value_from_number() {
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let session = fixture.session();
        let mut builder = session.build_value();
        builder.push_op(42, ());
        let (_, value) = builder.finish();
        assert_eq!(value, 42.into())
    }

    #[test]
    fn apply_operator_on_two_values() {
        let log = RefCell::new(Vec::new());
        let mut fixture = Fixture::new(&log);
        let session = fixture.session();
        let mut builder = session.build_value();
        builder.push_op(42, ());
        builder.push_op(Ident::from("ident"), ());
        builder.push_op(BinOp::Multiplication, ());
        let (_, value) = builder.finish();
        assert_eq!(
            value,
            Expr::from_items(&[
                42.into(),
                Atom::Name(0).into(),
                BinOp::Multiplication.into()
            ])
        )
    }

    type MockAnalyzer<'a, S> = crate::analysis::semantics::MockAnalyzer<'a, Event<S>>;
    type MockBackend<'a, S> = crate::analysis::backend::MockBackend<'a, Event<S>>;
    type MockDiagnosticsSystem<'a, S> = crate::diag::MockDiagnosticsSystem<'a, Event<S>, S>;
    type MockNameTable<'a, S> = crate::analysis::resolve::MockNameTable<
        'a,
        BasicNameTable<usize, MacroEntry<String, MockDiagnosticsSystem<'a, S>>>,
        Event<S>,
    >;
    type TestSession<'a, 'b, S> = CompositeSession<
        'b,
        'b,
        MockCodebase<S>,
        MockAnalyzer<'a, S>,
        MockBackend<'a, S>,
        MockNameTable<'a, S>,
        MockDiagnosticsSystem<'a, S>,
    >;

    #[derive(Debug, PartialEq)]
    enum Event<S: Clone> {
        Frontend(AnalyzerEvent<S>),
        Backend(BackendEvent<Expr<S>>),
        NameTable(NameTableEvent),
        Diagnostics(DiagnosticsEvent<S>),
    }

    impl<S: Clone> From<AnalyzerEvent<S>> for Event<S> {
        fn from(event: AnalyzerEvent<S>) -> Self {
            Event::Frontend(event)
        }
    }

    impl<S: Clone> From<BackendEvent<Expr<S>>> for Event<S> {
        fn from(event: BackendEvent<Expr<S>>) -> Self {
            Event::Backend(event)
        }
    }

    impl<S: Clone> From<NameTableEvent> for Event<S> {
        fn from(event: NameTableEvent) -> Self {
            Event::NameTable(event)
        }
    }

    impl<S: Clone> From<DiagnosticsEvent<S>> for Event<S> {
        fn from(event: DiagnosticsEvent<S>) -> Self {
            Event::Diagnostics(event)
        }
    }

    struct Fixture<'a, S: Clone + Default + Merge> {
        codebase: MockCodebase<S>,
        analyzer: MockAnalyzer<'a, S>,
        backend: Option<MockBackend<'a, S>>,
        names: MockNameTable<'a, S>,
        diagnostics: MockDiagnosticsSystem<'a, S>,
    }

    impl<'a, S: Clone + Default + Merge> Fixture<'a, S> {
        fn new(log: &'a RefCell<Vec<Event<S>>>) -> Self {
            Self {
                codebase: MockCodebase::new(),
                analyzer: MockAnalyzer::new(log),
                backend: Some(MockBackend::new(log)),
                names: MockNameTable::new(BasicNameTable::new(), log),
                diagnostics: MockDiagnosticsSystem::new(log),
            }
        }

        fn session<'b>(&'b mut self) -> TestSession<'a, 'b, S> {
            CompositeSession::new(
                &mut self.codebase,
                &mut self.analyzer,
                self.backend.take().unwrap(),
                &mut self.names,
                &mut self.diagnostics,
            )
        }
    }
}
