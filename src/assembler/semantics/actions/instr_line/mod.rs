use self::label::{LabelSemantics, LabelState};
use self::macro_instr::{MacroInstrSemantics, MacroInstrState};

use super::{Keyword, Semantics, TokenStreamSemantics};

use crate::assembler::semantics::*;
use crate::assembler::session::resolve::ResolvedName;
use crate::assembler::syntax::actions::{InstrContext, InstrLineContext, InstrRule};
use crate::diagnostics::Message;
use crate::expr::{Atom, Expr, ExprOp};
use crate::span::WithSpan;

mod builtin_instr;
mod label;
mod macro_instr;

impl<'a, S: Analysis> InstrLineContext for InstrLineSemantics<'a, S>
where
    S::StringRef: 'static,
    S::Span: 'static,
{
    type LabelContext = LabelSemantics<'a, S>;
    type InstrContext = Self;

    fn will_parse_label(mut self, label: (S::StringRef, S::Span)) -> Self::LabelContext {
        self.flush_label();
        self.map_state(|line| LabelState::new(line, label))
    }
}

impl<'a, S: Analysis> InstrContext for InstrLineSemantics<'a, S>
where
    S::StringRef: 'static,
    S::Span: 'static,
{
    type BuiltinInstrContext = BuiltinInstrSemantics<'a, S>;
    type MacroInstrContext = MacroInstrSemantics<'a, S>;
    type ErrorContext = Self;
    type LineFinalizer = TokenStreamSemantics<'a, S>;

    fn will_parse_instr(
        mut self,
        ident: S::StringRef,
        span: S::Span,
    ) -> InstrRule<Self::BuiltinInstrContext, Self::MacroInstrContext, Self> {
        match self.session.resolve_name(&ident) {
            Some(ResolvedName::Keyword(Keyword::BuiltinMnemonic(mnemonic))) => {
                if !mnemonic.binds_to_label() {
                    self.flush_label();
                }
                InstrRule::BuiltinInstr(set_state!(
                    self,
                    BuiltinInstrState::new(self.state.label, mnemonic.clone().with_span(span))
                ))
            }
            Some(ResolvedName::Macro(id)) => {
                self.flush_label();
                InstrRule::MacroInstr(set_state!(
                    self,
                    MacroInstrState::new(self.state, (id, span))
                ))
            }
            Some(ResolvedName::Symbol(_)) => {
                let name = self.session.strip_span(&span);
                self.session
                    .emit_diag(Message::CannotUseSymbolNameAsMacroName { name }.at(span));
                InstrRule::Error(self)
            }
            Some(ResolvedName::Keyword(Keyword::Operand(_))) | None => {
                let name = self.session.strip_span(&span);
                self.session
                    .emit_diag(Message::NotAMnemonic { name }.at(span));
                InstrRule::Error(self)
            }
        }
    }
}

impl<'a, S: Analysis> InstrLineSemantics<'a, S> {
    pub fn flush_label(&mut self) {
        if let Some(((label, span), _params)) = self.state.label.take() {
            if self.session.name_visibility(&label) == Visibility::Global {
                self.session.start_scope();
            }
            let id = self.reloc_lookup(label, span.clone());
            self.session.define_symbol(
                id,
                span.clone(),
                Expr(vec![ExprOp::Atom(Atom::Location).with_span(span)]),
            );
        }
    }
}

impl<'a, S, T> Semantics<'a, S, T>
where
    S: Analysis,
{
    fn reloc_lookup(&mut self, name: S::StringRef, span: S::Span) -> S::SymbolId {
        match self.session.resolve_name(&name) {
            Some(ResolvedName::Keyword(_)) => unimplemented!(),
            Some(ResolvedName::Symbol(id)) => id,
            None => {
                let id = self.session.alloc_symbol(span);
                self.session
                    .define_name(name, ResolvedName::Symbol(id.clone()));
                id
            }
            Some(ResolvedName::Macro(_)) => {
                self.session
                    .emit_diag(Message::MacroNameInExpr.at(span.clone()));
                self.session.alloc_symbol(span)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assembler::semantics::actions::tests::*;
    use crate::assembler::session::mock::MockSymbolId;
    use crate::assembler::syntax::actions::*;
    use crate::diagnostics::{DiagnosticsEvent, Message, MockSpan};

    #[test]
    fn diagnose_unknown_mnemonic() {
        let name = "unknown";
        let log = collect_semantic_actions::<_, MockSpan<_>>(|session| {
            session
                .will_parse_line()
                .into_instr_line()
                .will_parse_instr(name.into(), name.into())
                .error()
                .unwrap()
                .did_parse_instr()
                .did_parse_line("eol".into())
                .act_on_eos("eos".into())
        });
        assert_eq!(
            log,
            [DiagnosticsEvent::EmitDiag(
                Message::NotAMnemonic { name: name.into() }
                    .at(name.into())
                    .into()
            )
            .into()]
        )
    }

    #[test]
    fn diagnose_operand_as_mnemonic() {
        let name = "HL";
        let log = collect_semantic_actions::<_, MockSpan<_>>(|session| {
            session
                .will_parse_line()
                .into_instr_line()
                .will_parse_instr(name.into(), name.into())
                .error()
                .unwrap()
                .did_parse_instr()
                .did_parse_line("eol".into())
                .act_on_eos("eos".into())
        });
        assert_eq!(
            log,
            [DiagnosticsEvent::EmitDiag(
                Message::NotAMnemonic { name: name.into() }
                    .at(name.into())
                    .into()
            )
            .into()]
        )
    }

    #[test]
    fn diagnose_symbol_as_mnemonic() {
        let name = "symbol";
        let log = log_with_predefined_names::<_, _, MockSpan<_>>(
            vec![(name.into(), ResolvedName::Symbol(MockSymbolId(42)))],
            |session| {
                session
                    .will_parse_line()
                    .into_instr_line()
                    .will_parse_instr(name.into(), name.into())
                    .error()
                    .unwrap()
                    .did_parse_line("eol".into())
                    .act_on_eos("eos".into())
            },
        );
        assert_eq!(
            log,
            [DiagnosticsEvent::EmitDiag(
                Message::CannotUseSymbolNameAsMacroName { name: name.into() }
                    .at(name.into())
                    .into()
            )
            .into()]
        )
    }
}
