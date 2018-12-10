use crate::backend::{self, Backend, BinaryOperator, ValueBuilder};
use crate::diagnostics::{
    Diagnostics, DiagnosticsListener, DownstreamDiagnostics, InternalDiagnostic, Message,
};
use crate::expr::ExprVariant;
use crate::frontend::session::Session;
use crate::frontend::syntax::{self, keyword::*, ExprAtom, ExprOperator, Token};
use crate::frontend::{Frontend, Literal};
use crate::span::{Merge, Source, Span};

mod directive;
mod instruction;
mod operand;

mod expr {
    use crate::expr::Expr;
    #[cfg(test)]
    use crate::expr::ExprVariant;
    use crate::frontend::Literal;

    #[derive(Debug, PartialEq)]
    pub enum SemanticAtom<I> {
        Ident(I),
        Literal(Literal<I>),
    }

    impl<I> From<Literal<I>> for SemanticAtom<I> {
        fn from(literal: Literal<I>) -> Self {
            SemanticAtom::Literal(literal)
        }
    }

    #[derive(Debug, PartialEq)]
    pub enum SemanticUnary {
        Parentheses,
    }

    #[derive(Debug, PartialEq)]
    pub enum SemanticBinary {
        Plus,
    }

    pub type SemanticExpr<I, S> = Expr<SemanticAtom<I>, SemanticUnary, SemanticBinary, S>;

    #[cfg(test)]
    pub type SemanticExprVariant<I, S> =
        ExprVariant<SemanticAtom<I>, SemanticUnary, SemanticBinary, S>;
}

use self::expr::*;

pub struct SemanticActions<'a, F: Frontend<D>, B, D: Diagnostics> {
    session: Session<'a, F, B, D>,
    label: Option<(F::Ident, D::Span)>,
}

impl<'a, F: Frontend<D>, B: Backend<D::Span>, D: Diagnostics> SemanticActions<'a, F, B, D> {
    pub fn new(session: Session<'a, F, B, D>) -> SemanticActions<'a, F, B, D> {
        SemanticActions {
            session,
            label: None,
        }
    }

    fn define_label_if_present(&mut self) {
        if let Some((label, span)) = self.label.take() {
            let value = self.session.backend.build_value().location(span.clone());
            self.session
                .backend
                .define_symbol((label.into(), span), value)
        }
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Span for SemanticActions<'a, F, B, D> {
    type Span = D::Span;
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> DownstreamDiagnostics for SemanticActions<'a, F, B, D> {
    type Output = D::Output;
    fn diagnostics(&mut self) -> &mut Self::Output {
        self.session.diagnostics.diagnostics()
    }
}

impl<'a, F, B, D> syntax::FileContext<F::Ident, Command, Literal<F::Ident>>
    for SemanticActions<'a, F, B, D>
where
    F: Frontend<D>,
    B: Backend<D::Span>,
    D: Diagnostics,
{
    type StmtContext = Self;

    fn enter_stmt(mut self, label: Option<(F::Ident, D::Span)>) -> Self::StmtContext {
        self.label = label;
        self
    }
}

impl<'a, F, B, D> syntax::StmtContext<F::Ident, Command, Literal<F::Ident>>
    for SemanticActions<'a, F, B, D>
where
    F: Frontend<D>,
    B: Backend<D::Span>,
    D: Diagnostics,
{
    type CommandContext = CommandActions<'a, F, B, D>;
    type MacroParamsContext = MacroDefActions<'a, F, B, D>;
    type MacroInvocationContext = MacroInvocationActions<'a, F, B, D>;
    type Parent = Self;

    fn enter_command(self, name: (Command, D::Span)) -> Self::CommandContext {
        CommandActions::new(name, self)
    }

    fn enter_macro_def(mut self, keyword: D::Span) -> Self::MacroParamsContext {
        if self.label.is_none() {
            self.diagnostics()
                .emit_diagnostic(InternalDiagnostic::new(Message::MacroRequiresName, keyword))
        }
        MacroDefActions::new(self.label.take(), self)
    }

    fn enter_macro_invocation(mut self, name: (F::Ident, D::Span)) -> Self::MacroInvocationContext {
        self.define_label_if_present();
        MacroInvocationActions::new(name, self)
    }

    fn exit(mut self) -> Self::Parent {
        self.define_label_if_present();
        self
    }
}

pub struct CommandActions<'a, F: Frontend<D>, B, D: Diagnostics> {
    name: (Command, D::Span),
    args: CommandArgs<F::Ident, D::Span>,
    parent: SemanticActions<'a, F, B, D>,
    has_errors: bool,
}

type CommandArgs<I, S> = Vec<SemanticExpr<I, S>>;

impl<'a, F: Frontend<D>, B, D: Diagnostics> CommandActions<'a, F, B, D> {
    fn new(
        name: (Command, D::Span),
        parent: SemanticActions<'a, F, B, D>,
    ) -> CommandActions<'a, F, B, D> {
        CommandActions {
            name,
            args: Vec::new(),
            parent,
            has_errors: false,
        }
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Span for CommandActions<'a, F, B, D> {
    type Span = D::Span;
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Merge for CommandActions<'a, F, B, D> {
    fn merge(&mut self, left: &Self::Span, right: &Self::Span) -> Self::Span {
        self.parent.diagnostics().merge(left, right)
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> DiagnosticsListener for CommandActions<'a, F, B, D> {
    fn emit_diagnostic(&mut self, diagnostic: InternalDiagnostic<D::Span>) {
        self.has_errors = true;
        self.parent.diagnostics().emit_diagnostic(diagnostic)
    }
}

impl<'a, F: Frontend<D>, B: Backend<D::Span>, D: Diagnostics> syntax::CommandContext
    for CommandActions<'a, F, B, D>
{
    type Ident = F::Ident;
    type Command = Command;
    type Literal = Literal<F::Ident>;
    type ArgContext = ExprContext<'a, F, B, D>;
    type Parent = SemanticActions<'a, F, B, D>;

    fn add_argument(self) -> Self::ArgContext {
        ExprContext {
            stack: Vec::new(),
            parent: self,
        }
    }

    fn exit(mut self) -> Self::Parent {
        if !self.has_errors {
            let result = match self.name {
                (Command::Directive(directive), span) => {
                    if !directive.requires_symbol() {
                        self.parent.define_label_if_present()
                    }
                    directive::analyze_directive((directive, span), self.args, &mut self.parent)
                }
                (Command::Mnemonic(mnemonic), range) => {
                    self.parent.define_label_if_present();
                    analyze_mnemonic((mnemonic, range), self.args, &mut self.parent)
                }
            };
            if let Err(diagnostic) = result {
                self.parent.diagnostics().emit_diagnostic(diagnostic);
            }
        }
        self.parent
    }
}

impl Directive {
    fn requires_symbol(&self) -> bool {
        match self {
            Directive::Equ => true,
            _ => false,
        }
    }
}

pub struct ExprContext<'a, F: Frontend<D>, B, D: Diagnostics> {
    stack: Vec<SemanticExpr<F::Ident, D::Span>>,
    parent: CommandActions<'a, F, B, D>,
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Span for ExprContext<'a, F, B, D> {
    type Span = D::Span;
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> DownstreamDiagnostics for ExprContext<'a, F, B, D> {
    type Output = CommandActions<'a, F, B, D>;
    fn diagnostics(&mut self) -> &mut Self::Output {
        &mut self.parent
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> syntax::ExprContext for ExprContext<'a, F, B, D> {
    type Ident = F::Ident;
    type Literal = Literal<F::Ident>;
    type Parent = CommandActions<'a, F, B, D>;

    fn push_atom(&mut self, atom: (ExprAtom<Self::Ident, Self::Literal>, D::Span)) {
        self.stack.push(SemanticExpr {
            variant: ExprVariant::Atom(match atom.0 {
                ExprAtom::Ident(ident) => SemanticAtom::Ident(ident),
                ExprAtom::Literal(literal) => SemanticAtom::Literal(literal),
            }),
            span: atom.1,
        })
    }

    fn apply_operator(&mut self, operator: (ExprOperator, D::Span)) {
        match operator.0 {
            ExprOperator::Parentheses => {
                let inner = self.stack.pop().unwrap_or_else(|| unreachable!());
                self.stack.push(SemanticExpr {
                    variant: ExprVariant::Unary(SemanticUnary::Parentheses, Box::new(inner)),
                    span: operator.1,
                })
            }
            ExprOperator::Plus => {
                let rhs = self.stack.pop().unwrap_or_else(|| unreachable!());
                let lhs = self.stack.pop().unwrap_or_else(|| unreachable!());
                self.stack.push(SemanticExpr {
                    variant: ExprVariant::Binary(
                        SemanticBinary::Plus,
                        Box::new(lhs),
                        Box::new(rhs),
                    ),
                    span: operator.1,
                })
            }
        }
    }

    fn exit(mut self) -> Self::Parent {
        if !self.parent.has_errors {
            assert_eq!(self.stack.len(), 1);
            self.parent.args.push(self.stack.pop().unwrap());
        }
        self.parent
    }
}

fn analyze_mnemonic<'a, F: Frontend<D>, B: Backend<D::Span>, D: Diagnostics>(
    name: (Mnemonic, D::Span),
    args: CommandArgs<F::Ident, D::Span>,
    actions: &mut SemanticActions<'a, F, B, D>,
) -> Result<(), InternalDiagnostic<D::Span>> {
    let instruction = instruction::analyze_instruction(
        name,
        args.into_iter(),
        &mut actions.session.backend.build_value(),
        actions.session.diagnostics.diagnostics(),
    )?;
    actions
        .session
        .backend
        .emit_item(backend::Item::Instruction(instruction));
    Ok(())
}

pub struct MacroDefActions<'a, F: Frontend<D>, B, D: Diagnostics> {
    name: Option<(F::Ident, D::Span)>,
    params: Vec<(F::Ident, D::Span)>,
    tokens: Vec<(Token<F::Ident>, D::Span)>,
    parent: SemanticActions<'a, F, B, D>,
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> MacroDefActions<'a, F, B, D> {
    fn new(
        name: Option<(F::Ident, D::Span)>,
        parent: SemanticActions<'a, F, B, D>,
    ) -> MacroDefActions<'a, F, B, D> {
        MacroDefActions {
            name,
            params: Vec::new(),
            tokens: Vec::new(),
            parent,
        }
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Span for MacroDefActions<'a, F, B, D> {
    type Span = D::Span;
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> DownstreamDiagnostics for MacroDefActions<'a, F, B, D> {
    type Output = D::Output;
    fn diagnostics(&mut self) -> &mut Self::Output {
        self.parent.diagnostics()
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> syntax::MacroParamsContext
    for MacroDefActions<'a, F, B, D>
{
    type Ident = F::Ident;
    type Command = Command;
    type Literal = Literal<F::Ident>;
    type MacroBodyContext = Self;
    type Parent = SemanticActions<'a, F, B, D>;

    fn add_parameter(&mut self, param: (Self::Ident, D::Span)) {
        self.params.push(param)
    }

    fn exit(self) -> Self::MacroBodyContext {
        self
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> syntax::TokenSeqContext
    for MacroDefActions<'a, F, B, D>
{
    type Token = Token<F::Ident>;
    type Parent = SemanticActions<'a, F, B, D>;

    fn push_token(&mut self, token: (Self::Token, D::Span)) {
        self.tokens.push(token)
    }

    fn exit(mut self) -> Self::Parent {
        if let Some(name) = self.name {
            self.parent
                .session
                .define_macro(name, self.params, self.tokens)
        }
        self.parent
    }
}

pub struct MacroInvocationActions<'a, F: Frontend<D>, B, D: Diagnostics> {
    name: (F::Ident, D::Span),
    args: Vec<super::TokenSeq<F::Ident, D::Span>>,
    parent: SemanticActions<'a, F, B, D>,
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> MacroInvocationActions<'a, F, B, D> {
    fn new(
        name: (F::Ident, D::Span),
        parent: SemanticActions<'a, F, B, D>,
    ) -> MacroInvocationActions<'a, F, B, D> {
        MacroInvocationActions {
            name,
            args: Vec::new(),
            parent,
        }
    }

    fn push_arg(&mut self, arg: Vec<(Token<F::Ident>, D::Span)>) {
        self.args.push(arg)
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Span for MacroInvocationActions<'a, F, B, D> {
    type Span = D::Span;
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> DownstreamDiagnostics
    for MacroInvocationActions<'a, F, B, D>
{
    type Output = D::Output;
    fn diagnostics(&mut self) -> &mut Self::Output {
        self.parent.diagnostics()
    }
}

impl<'a, F: Frontend<D>, B: Backend<D::Span>, D: Diagnostics> syntax::MacroInvocationContext
    for MacroInvocationActions<'a, F, B, D>
{
    type Token = Token<F::Ident>;
    type Parent = SemanticActions<'a, F, B, D>;
    type MacroArgContext = MacroArgContext<'a, F, B, D>;

    fn enter_macro_arg(self) -> Self::MacroArgContext {
        MacroArgContext::new(self)
    }

    fn exit(mut self) -> Self::Parent {
        self.parent.session.invoke_macro(self.name, self.args);
        self.parent
    }
}

pub struct MacroArgContext<'a, F: Frontend<D>, B, D: Diagnostics> {
    tokens: Vec<(Token<F::Ident>, D::Span)>,
    parent: MacroInvocationActions<'a, F, B, D>,
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> MacroArgContext<'a, F, B, D> {
    fn new(parent: MacroInvocationActions<'a, F, B, D>) -> MacroArgContext<'a, F, B, D> {
        MacroArgContext {
            tokens: Vec::new(),
            parent,
        }
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> Span for MacroArgContext<'a, F, B, D> {
    type Span = D::Span;
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> DownstreamDiagnostics for MacroArgContext<'a, F, B, D> {
    type Output = D::Output;
    fn diagnostics(&mut self) -> &mut Self::Output {
        self.parent.parent.diagnostics()
    }
}

impl<'a, F: Frontend<D>, B, D: Diagnostics> syntax::TokenSeqContext
    for MacroArgContext<'a, F, B, D>
{
    type Token = Token<F::Ident>;
    type Parent = MacroInvocationActions<'a, F, B, D>;

    fn push_token(&mut self, token: (Self::Token, D::Span)) {
        self.tokens.push(token)
    }

    fn exit(mut self) -> Self::Parent {
        self.parent.push_arg(self.tokens);
        self.parent
    }
}

fn analyze_reloc_expr<I: Into<String>, V: Source>(
    expr: SemanticExpr<I, V::Span>,
    builder: &mut impl ValueBuilder<V>,
) -> Result<V, InternalDiagnostic<V::Span>> {
    match expr.variant {
        ExprVariant::Atom(SemanticAtom::Ident(ident)) => {
            Ok(builder.symbol((ident.into(), expr.span)))
        }
        ExprVariant::Atom(SemanticAtom::Literal(Literal::Number(n))) => {
            Ok(builder.number((n, expr.span)))
        }
        ExprVariant::Atom(SemanticAtom::Literal(Literal::Operand(_))) => {
            Err(InternalDiagnostic::new(
                Message::KeywordInExpr {
                    keyword: expr.span.clone(),
                },
                expr.span,
            ))
        }
        ExprVariant::Atom(SemanticAtom::Literal(Literal::String(_))) => Err(
            InternalDiagnostic::new(Message::StringInInstruction, expr.span),
        ),
        ExprVariant::Unary(SemanticUnary::Parentheses, expr) => analyze_reloc_expr(*expr, builder),
        ExprVariant::Binary(SemanticBinary::Plus, left, right) => {
            let left = analyze_reloc_expr(*left, builder)?;
            let right = analyze_reloc_expr(*right, builder)?;
            Ok(builder.apply_binary_operator((BinaryOperator::Plus, expr.span), left, right))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::{BuildValue, HasValue, RelocAtom, RelocExpr, RelocExprBuilder, Width};
    use crate::codebase::{BufId, BufRange, CodebaseError};
    use crate::diagnostics::{InternalDiagnostic, Message};
    use crate::frontend::syntax::{
        keyword::Operand, CommandContext, ExprContext, FileContext, MacroInvocationContext,
        MacroParamsContext, StmtContext, TokenSeqContext,
    };
    use crate::frontend::{Downstream, MacroArgs};
    use crate::span::*;
    use std::borrow::Borrow;
    use std::cell::RefCell;

    pub struct TestFrontend<'a> {
        operations: &'a RefCell<Vec<TestOperation>>,
        error: Option<CodebaseError>,
    }

    impl<'a> TestFrontend<'a> {
        pub fn new(operations: &'a RefCell<Vec<TestOperation>>) -> Self {
            TestFrontend {
                operations,
                error: None,
            }
        }

        pub fn fail(&mut self, error: CodebaseError) {
            self.error = Some(error)
        }
    }

    impl<'a> Frontend<TestDiagnostics<'a>> for TestFrontend<'a> {
        type Ident = String;
        type MacroDefId = usize;

        fn analyze_file<B>(
            &mut self,
            path: Self::Ident,
            _downstream: Downstream<B, TestDiagnostics<'a>>,
        ) -> Result<(), CodebaseError>
        where
            B: Backend<()>,
        {
            self.operations
                .borrow_mut()
                .push(TestOperation::AnalyzeFile(path));
            match self.error.take() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }

        fn invoke_macro<B>(
            &mut self,
            name: (Self::Ident, ()),
            args: MacroArgs<Self::Ident, ()>,
            _downstream: Downstream<B, TestDiagnostics<'a>>,
        ) where
            B: Backend<()>,
        {
            self.operations
                .borrow_mut()
                .push(TestOperation::InvokeMacro(
                    name.0,
                    args.into_iter()
                        .map(|arg| arg.into_iter().map(|(token, _)| token).collect())
                        .collect(),
                ))
        }

        fn define_macro(
            &mut self,
            name: (impl Into<Self::Ident>, ()),
            params: Vec<(Self::Ident, ())>,
            tokens: Vec<(Token<Self::Ident>, ())>,
            _diagnostics: &mut TestDiagnostics<'a>,
        ) {
            self.operations
                .borrow_mut()
                .push(TestOperation::DefineMacro(
                    name.0.into(),
                    params.into_iter().map(|(s, _)| s).collect(),
                    tokens.into_iter().map(|(t, _)| t).collect(),
                ))
        }
    }

    pub struct TestBackend<'a> {
        operations: &'a RefCell<Vec<TestOperation>>,
    }

    impl<'a> TestBackend<'a> {
        pub fn new(operations: &'a RefCell<Vec<TestOperation>>) -> Self {
            TestBackend { operations }
        }
    }

    impl<'a> Span for TestBackend<'a> {
        type Span = ();
    }

    impl<'a> HasValue for TestBackend<'a> {
        type Value = RelocExpr<()>;
    }

    impl<'a, 'b> BuildValue<'b, RelocExpr<()>> for TestBackend<'a> {
        type Builder = RelocExprBuilder<()>;

        fn build_value(&mut self) -> Self::Builder {
            RelocExprBuilder::new()
        }
    }

    impl<'a> Backend<()> for TestBackend<'a> {
        type Object = ();

        fn define_symbol(&mut self, symbol: (impl Into<String>, ()), value: RelocExpr<()>) {
            self.operations
                .borrow_mut()
                .push(TestOperation::DefineSymbol(symbol.0.into(), value))
        }

        fn emit_item(&mut self, item: backend::Item<RelocExpr<()>>) {
            self.operations
                .borrow_mut()
                .push(TestOperation::EmitItem(item))
        }

        fn into_object(self) -> Self::Object {}

        fn set_origin(&mut self, origin: RelocExpr<()>) {
            self.operations
                .borrow_mut()
                .push(TestOperation::SetOrigin(origin))
        }
    }

    pub struct TestDiagnostics<'a> {
        operations: &'a RefCell<Vec<TestOperation>>,
    }

    impl<'a> TestDiagnostics<'a> {
        pub fn new(operations: &'a RefCell<Vec<TestOperation>>) -> Self {
            TestDiagnostics { operations }
        }
    }

    impl<'a> Diagnostics for TestDiagnostics<'a> {}

    impl<'a> Span for TestDiagnostics<'a> {
        type Span = ();
    }

    impl<'a> Merge for TestDiagnostics<'a> {
        fn merge(&mut self, _: &(), _: &()) {}
    }

    impl<'a> DiagnosticsListener for TestDiagnostics<'a> {
        fn emit_diagnostic(&mut self, diagnostic: InternalDiagnostic<Self::Span>) {
            self.operations
                .borrow_mut()
                .push(TestOperation::EmitDiagnostic(diagnostic))
        }
    }

    impl<'a> MacroContextFactory for TestDiagnostics<'a> {
        type MacroDefId = usize;
        type MacroExpansionContext = TestMacroExpansionContext;

        fn add_macro_def<P, B>(
            &mut self,
            _name: Self::Span,
            _params: P,
            _body: B,
        ) -> Self::MacroDefId
        where
            P: IntoIterator<Item = Self::Span>,
            B: IntoIterator<Item = Self::Span>,
        {
            0
        }

        fn mk_macro_expansion_context<A, J>(
            &mut self,
            _name: Self::Span,
            _args: A,
            _def: &Self::MacroDefId,
        ) -> Self::MacroExpansionContext
        where
            A: IntoIterator<Item = J>,
            J: IntoIterator<Item = Self::Span>,
        {
            TestMacroExpansionContext
        }
    }

    pub struct TestMacroExpansionContext;

    impl MacroExpansionContext for TestMacroExpansionContext {
        type Span = ();

        fn mk_span(&self, _token: usize, _expansion: Option<TokenExpansion>) -> Self::Span {}
    }

    impl<'a> ContextFactory for TestDiagnostics<'a> {
        type BufContext = TestBufContext;

        fn mk_buf_context(
            &mut self,
            _buf_id: BufId,
            _included_from: Option<Self::Span>,
        ) -> Self::BufContext {
            TestBufContext
        }
    }

    pub struct TestBufContext;

    impl BufContext for TestBufContext {
        type Span = ();

        fn mk_span(&self, _range: BufRange) -> Self::Span {}
    }

    #[derive(Debug, PartialEq)]
    pub enum TestOperation {
        AnalyzeFile(String),
        InvokeMacro(String, Vec<Vec<Token<String>>>),
        DefineMacro(String, Vec<String>, Vec<Token<String>>),
        DefineSymbol(String, RelocExpr<()>),
        EmitDiagnostic(InternalDiagnostic<()>),
        EmitItem(backend::Item<RelocExpr<()>>),
        SetOrigin(RelocExpr<()>),
    }

    #[test]
    fn emit_ld_b_deref_hl() {
        use crate::instruction::*;
        let actions = collect_semantic_actions(|actions| {
            let mut command = actions
                .enter_stmt(None)
                .enter_command((Command::Mnemonic(Mnemonic::Ld), ()));
            let mut arg1 = command.add_argument();
            arg1.push_atom((ExprAtom::Literal(Literal::Operand(Operand::B)), ()));
            command = arg1.exit();
            let mut arg2 = command.add_argument();
            arg2.push_atom((ExprAtom::Literal(Literal::Operand(Operand::Hl)), ()));
            arg2.apply_operator((ExprOperator::Parentheses, ()));
            arg2.exit().exit().exit()
        });
        assert_eq!(
            actions,
            [TestOperation::EmitItem(backend::Item::Instruction(
                Instruction::Ld(Ld::Simple(SimpleOperand::B, SimpleOperand::DerefHl))
            ))]
        )
    }

    #[test]
    fn emit_rst_1_plus_1() {
        use crate::instruction::*;
        let actions = collect_semantic_actions(|actions| {
            let command = actions
                .enter_stmt(None)
                .enter_command((Command::Mnemonic(Mnemonic::Rst), ()));
            let mut expr = command.add_argument();
            expr.push_atom((ExprAtom::Literal(Literal::Number(1)), ()));
            expr.push_atom((ExprAtom::Literal(Literal::Number(1)), ()));
            expr.apply_operator((ExprOperator::Plus, ()));
            expr.exit().exit().exit()
        });
        assert_eq!(
            actions,
            [TestOperation::EmitItem(backend::Item::Instruction(
                Instruction::Rst(
                    ExprVariant::Binary(
                        BinaryOperator::Plus,
                        Box::new(1.into()),
                        Box::new(1.into()),
                    )
                    .into()
                )
            ))]
        )
    }

    #[test]
    fn emit_label_word() {
        let label = "my_label";
        let actions = collect_semantic_actions(|actions| {
            let mut arg = actions
                .enter_stmt(None)
                .enter_command((Command::Directive(Directive::Dw), ()))
                .add_argument();
            arg.push_atom((ExprAtom::Ident(label.to_string()), ()));
            arg.exit().exit().exit()
        });
        assert_eq!(
            actions,
            [TestOperation::EmitItem(backend::Item::Data(
                RelocAtom::Symbol(label.to_string()).into(),
                Width::Word
            ))]
        );
    }

    #[test]
    fn analyze_label() {
        let label = "label";
        let actions = collect_semantic_actions(|actions| {
            actions.enter_stmt(Some((label.to_string(), ()))).exit()
        });
        assert_eq!(
            actions,
            [TestOperation::DefineSymbol(
                label.to_string(),
                RelocAtom::LocationCounter.into()
            )]
        )
    }

    #[test]
    fn define_nullary_macro() {
        test_macro_definition(
            "my_macro",
            [],
            [
                Token::Command(Command::Mnemonic(Mnemonic::Xor)),
                Token::Literal(Literal::Operand(Operand::A)),
            ],
        )
    }

    #[test]
    fn define_unary_macro() {
        let param = "reg";
        test_macro_definition(
            "my_xor",
            [param],
            [
                Token::Command(Command::Mnemonic(Mnemonic::Xor)),
                Token::Ident(param.to_string()),
            ],
        )
    }

    #[test]
    fn define_nameless_macro() {
        let actions = collect_semantic_actions(|actions| {
            let params = actions.enter_stmt(None).enter_macro_def(());
            TokenSeqContext::exit(MacroParamsContext::exit(params))
        });
        assert_eq!(
            actions,
            [TestOperation::EmitDiagnostic(InternalDiagnostic::new(
                Message::MacroRequiresName,
                ()
            ))]
        )
    }

    fn test_macro_definition(
        name: &str,
        params: impl Borrow<[&'static str]>,
        body: impl Borrow<[Token<String>]>,
    ) {
        let actions = collect_semantic_actions(|actions| {
            let mut params_actions = actions
                .enter_stmt(Some((name.to_string(), ())))
                .enter_macro_def(());
            for param in params.borrow().iter().map(|t| (t.to_string(), ())) {
                params_actions.add_parameter(param)
            }
            let mut token_seq_actions = MacroParamsContext::exit(params_actions);
            for token in body.borrow().iter().cloned().map(|t| (t, ())) {
                token_seq_actions.push_token(token)
            }
            TokenSeqContext::exit(token_seq_actions)
        });
        assert_eq!(
            actions,
            [TestOperation::DefineMacro(
                name.to_string(),
                params.borrow().iter().cloned().map(String::from).collect(),
                body.borrow().iter().cloned().collect()
            )]
        )
    }

    #[test]
    fn invoke_nullary_macro() {
        let name = "my_macro";
        let actions = collect_semantic_actions(|actions| {
            let invocation = actions
                .enter_stmt(None)
                .enter_macro_invocation((name.to_string(), ()));
            invocation.exit().exit()
        });
        assert_eq!(
            actions,
            [TestOperation::InvokeMacro(name.to_string(), Vec::new())]
        )
    }

    #[test]
    fn invoke_unary_macro() {
        let name = "my_macro";
        let arg_token = Token::Literal(Literal::Operand(Operand::A));
        let actions = collect_semantic_actions(|actions| {
            let mut invocation = actions
                .enter_stmt(None)
                .enter_macro_invocation((name.to_string(), ()));
            invocation = {
                let mut arg = invocation.enter_macro_arg();
                arg.push_token((arg_token.clone(), ()));
                arg.exit()
            };
            invocation.exit().exit()
        });
        assert_eq!(
            actions,
            [TestOperation::InvokeMacro(
                name.to_string(),
                vec![vec![arg_token]]
            )]
        )
    }

    #[test]
    fn diagnose_wrong_operand_count() {
        let actions = collect_semantic_actions(|actions| {
            let mut arg = actions
                .enter_stmt(None)
                .enter_command((Command::Mnemonic(Mnemonic::Nop), ()))
                .add_argument();
            let literal_a = Literal::Operand(Operand::A);
            arg.push_atom((ExprAtom::Literal(literal_a), ()));
            arg.exit().exit().exit()
        });
        assert_eq!(
            actions,
            [TestOperation::EmitDiagnostic(InternalDiagnostic::new(
                Message::OperandCount {
                    actual: 1,
                    expected: 0
                },
                ()
            ))]
        )
    }

    #[test]
    fn diagnose_parsing_error() {
        let diagnostic = InternalDiagnostic::new(Message::UnexpectedToken { token: () }, ());
        let actions = collect_semantic_actions(|actions| {
            let mut stmt = actions.enter_stmt(None);
            stmt.diagnostics().emit_diagnostic(diagnostic.clone());
            stmt.exit()
        });
        assert_eq!(actions, [TestOperation::EmitDiagnostic(diagnostic)])
    }

    #[test]
    fn recover_from_malformed_expr() {
        let diagnostic = InternalDiagnostic::new(Message::UnexpectedToken { token: () }, ());
        let actions = collect_semantic_actions(|file| {
            let mut expr = file
                .enter_stmt(None)
                .enter_command((Command::Mnemonic(Mnemonic::Add), ()))
                .add_argument();
            expr.diagnostics().emit_diagnostic(diagnostic.clone());
            expr.exit().exit().exit()
        });
        assert_eq!(actions, [TestOperation::EmitDiagnostic(diagnostic)])
    }

    pub fn collect_semantic_actions<F>(f: F) -> Vec<TestOperation>
    where
        F: for<'a> FnOnce(TestSemanticActions<'a>) -> TestSemanticActions<'a>,
    {
        let operations = RefCell::new(Vec::new());
        let mut frontend = TestFrontend::new(&operations);
        let mut backend = TestBackend::new(&operations);
        let mut diagnostics = TestDiagnostics::new(&operations);
        let session = Session::new(&mut frontend, &mut backend, &mut diagnostics);
        f(SemanticActions::new(session));
        operations.into_inner()
    }

    type TestSemanticActions<'a> =
        SemanticActions<'a, TestFrontend<'a>, TestBackend<'a>, TestDiagnostics<'a>>;
}
