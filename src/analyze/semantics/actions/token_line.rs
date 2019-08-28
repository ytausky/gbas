use super::{Keyword, Session, TokenStreamSemantics};

use crate::analyze::reentrancy::ReentrancyActions;
use crate::analyze::semantics::resolve::{NameTable, ResolvedName, StartScope};
use crate::analyze::semantics::*;
use crate::analyze::syntax::actions::{LineFinalizer, TokenLineActions, TokenLineRule};
use crate::analyze::syntax::{Sigil, Token};
use crate::analyze::{Literal, SemanticToken};
use crate::object::builder::SymbolSource;

use std::ops::DerefMut;

pub(in crate::analyze) type TokenLineSemantics<R, N, B> = Session<R, N, B, TokenContext<R>>;

impl<R, N, B> TokenLineActions<R::Ident, Literal<R::StringRef>, R::Span>
    for TokenLineSemantics<R, N, B>
where
    R: ReentrancyActions,
    N: DerefMut,
    N::Target: StartScope<R::Ident>
        + NameTable<
            R::Ident,
            Keyword = &'static Keyword,
            MacroId = R::MacroId,
            SymbolId = B::SymbolId,
        >,
    B: SymbolSource,
{
    type ContextFinalizer = TokenContextFinalizationSemantics<R, N, B>;

    fn act_on_token(&mut self, token: SemanticToken<R::Ident, R::StringRef>, span: R::Span) {
        match &mut self.state {
            TokenContext::MacroDef(state) => {
                state.tokens.0.push(token);
                state.tokens.1.push(span);
            }
        }
    }

    fn act_on_ident(
        mut self,
        ident: R::Ident,
        span: R::Span,
    ) -> TokenLineRule<Self, Self::ContextFinalizer> {
        match &mut self.state {
            TokenContext::MacroDef(state) => {
                if ident.as_ref().eq_ignore_ascii_case("ENDM") {
                    state.tokens.0.push(Sigil::Eos.into());
                    state.tokens.1.push(span);
                    TokenLineRule::LineEnd(TokenContextFinalizationSemantics { parent: self })
                } else {
                    state.tokens.0.push(Token::Ident(ident));
                    state.tokens.1.push(span);
                    TokenLineRule::TokenSeq(self)
                }
            }
        }
    }
}

impl<R: ReentrancyActions, N, B> LineFinalizer<R::Span> for TokenLineSemantics<R, N, B> {
    type Next = TokenStreamSemantics<R, N, B>;

    fn did_parse_line(mut self, span: R::Span) -> Self::Next {
        match &mut self.state {
            TokenContext::MacroDef(state) => {
                state.tokens.0.push(Sigil::Eol.into());
                state.tokens.1.push(span);
            }
        }
        set_state!(self, self.state.into())
    }
}

pub(in crate::analyze) struct TokenContextFinalizationSemantics<R: ReentrancyActions, N, B> {
    parent: TokenLineSemantics<R, N, B>,
}

delegate_diagnostics! {
    {R: ReentrancyActions, N, B}, TokenContextFinalizationSemantics<R, N, B>, {parent}, R, R::Span
}

impl<R, N, B> LineFinalizer<R::Span> for TokenContextFinalizationSemantics<R, N, B>
where
    R: ReentrancyActions,
    N: DerefMut,
    N::Target: NameTable<R::Ident, MacroId = R::MacroId, SymbolId = B::SymbolId>,
    B: SymbolSource,
{
    type Next = TokenStreamSemantics<R, N, B>;

    fn did_parse_line(mut self, _: R::Span) -> Self::Next {
        match self.parent.state {
            TokenContext::MacroDef(state) => {
                if let Some((name, params)) = state.label {
                    let tokens = state.tokens;
                    let id = self.parent.reentrancy.define_macro(name.1, params, tokens);
                    self.parent
                        .names
                        .define_name(name.0, ResolvedName::Macro(id));
                }
            }
        }
        set_state!(self.parent, TokenStreamState::new())
    }
}
