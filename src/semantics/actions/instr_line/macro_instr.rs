use super::*;

use crate::analyze::{SemanticToken, TokenSeq};
use crate::semantics::actions::TokenStreamState;
use crate::session::reentrancy::MacroArgs;
use crate::syntax::actions::{InstrFinalizer, MacroArgContext, MacroInstrContext};

pub(super) type MacroInstrSemantics<'a, 'b, S> = Semantics<
    'a,
    'b,
    S,
    MacroInstrState<S>,
    <S as IdentSource>::Ident,
    <S as StringSource>::StringRef,
    <S as SpanSource>::Span,
>;

pub(crate) struct MacroInstrState<S: Analysis> {
    parent: InstrLineState<S::Ident, S::Span>,
    name: (S::MacroId, S::Span),
    args: MacroArgs<S::Ident, S::StringRef, S::Span>,
}

impl<S: Analysis> MacroInstrState<S> {
    pub fn new(parent: InstrLineState<S::Ident, S::Span>, name: (S::MacroId, S::Span)) -> Self {
        Self {
            parent,
            name,
            args: (Vec::new(), Vec::new()),
        }
    }

    fn push_arg(&mut self, arg: TokenSeq<S::Ident, S::StringRef, S::Span>) {
        let args = &mut self.args;
        args.0.push(arg.0);
        args.1.push(arg.1);
    }
}

impl<'a, 'b, S: Analysis> MacroInstrContext for MacroInstrSemantics<'a, 'b, S>
where
    S::Ident: 'static,
    S::StringRef: 'static,
    S::Span: 'static,
{
    type MacroArgContext = MacroArgSemantics<'a, 'b, S>;

    fn will_parse_macro_arg(self) -> Self::MacroArgContext {
        set_state!(self, MacroArgState::new(self.state))
    }
}

impl<'a, 'b, S: Analysis> InstrFinalizer for MacroInstrSemantics<'a, 'b, S>
where
    S::Ident: 'static,
    S::StringRef: 'static,
    S::Span: 'static,
{
    type Next = TokenStreamSemantics<'a, 'b, S>;

    fn did_parse_instr(self) -> Self::Next {
        self.session.expand_macro(self.state.name, self.state.args);
        Semantics {
            session: self.session,
            state: TokenStreamState::from(self.state.parent),
            tokens: self.tokens,
        }
    }
}

type MacroArgSemantics<'a, 'b, S> = Semantics<
    'a,
    'b,
    S,
    MacroArgState<S>,
    <S as IdentSource>::Ident,
    <S as StringSource>::StringRef,
    <S as SpanSource>::Span,
>;

pub(crate) struct MacroArgState<S: Analysis> {
    tokens: TokenSeq<S::Ident, S::StringRef, S::Span>,
    parent: MacroInstrState<S>,
}

impl<S: Analysis> MacroArgState<S> {
    fn new(parent: MacroInstrState<S>) -> Self {
        Self {
            tokens: (Vec::new(), Vec::new()),
            parent,
        }
    }
}

impl<'a, 'b, S: Analysis> MacroArgContext for MacroArgSemantics<'a, 'b, S> {
    type Next = MacroInstrSemantics<'a, 'b, S>;

    fn act_on_token(&mut self, token: (SemanticToken<S::Ident, S::StringRef>, S::Span)) {
        let tokens = &mut self.state.tokens;
        tokens.0.push(token.0);
        tokens.1.push(token.1);
    }

    fn did_parse_macro_arg(mut self) -> Self::Next {
        self.state.parent.push_arg(self.state.tokens);
        set_state!(self, self.state.parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::analyze::macros::mock::{MacroTableEvent, MockMacroId};
    use crate::semantics::actions::tests::*;
    use crate::session::resolve::ResolvedName;
    use crate::syntax::actions::{InstrContext, LineFinalizer, TokenStreamContext};
    use crate::syntax::Token;

    #[test]
    fn call_nullary_macro() {
        let name = "my_macro";
        let macro_id = MockMacroId(0);
        let log = log_with_predefined_names(
            vec![(name.into(), ResolvedName::Macro(macro_id))],
            |actions| {
                actions
                    .will_parse_line()
                    .into_instr_line()
                    .will_parse_instr(name.into(), ())
                    .into_macro_instr()
                    .did_parse_instr()
                    .did_parse_line(())
                    .act_on_eos(())
            },
        );
        assert_eq!(
            log,
            [MacroTableEvent::ExpandMacro(macro_id, Vec::new()).into()]
        )
    }

    #[test]
    fn call_unary_macro() {
        let name = "my_macro";
        let arg_token = Token::Ident("A".into());
        let macro_id = MockMacroId(0);
        let log = log_with_predefined_names(
            vec![(name.into(), ResolvedName::Macro(macro_id))],
            |actions| {
                let mut call = actions
                    .will_parse_line()
                    .into_instr_line()
                    .will_parse_instr(name.into(), ())
                    .into_macro_instr();
                call = {
                    let mut arg = call.will_parse_macro_arg();
                    arg.act_on_token((arg_token.clone(), ()));
                    arg.did_parse_macro_arg()
                };
                call.did_parse_instr().did_parse_line(()).act_on_eos(())
            },
        );
        assert_eq!(
            log,
            [MacroTableEvent::ExpandMacro(macro_id, vec![vec![arg_token]]).into()]
        )
    }
}
