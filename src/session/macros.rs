use super::lex::{Lex, Literal, StringSource};
use super::NextToken;

use crate::codebase::{BufId, BufRange, Codebase};
use crate::semantics::{Semantics, TokenStreamState};
use crate::session::builder::Backend;
use crate::session::diagnostics::EmitDiag;
use crate::session::lex::LexItem;
use crate::session::resolve::Ident;
use crate::session::resolve::{NameTable, StartScope};
use crate::session::{Interner, TokenStream};
use crate::span::*;
use crate::syntax::parser::{DefaultParserFactory, ParseTokenStream, ParserFactory};
use crate::syntax::IdentSource;
use crate::syntax::LexError;
use crate::syntax::Token;
use crate::CompositeSession;

use std::fmt::Debug;
use std::rc::Rc;

pub(crate) trait MacroSource {
    type MacroId: Clone;
}

pub(crate) trait MacroTable<I, L, S: Clone>: MacroSource {
    fn define_macro(
        &mut self,
        name_span: S,
        params: (Vec<I>, Vec<S>),
        body: (Vec<Token<I, L>>, Vec<S>),
    ) -> Self::MacroId;

    fn expand_macro(&mut self, name: (Self::MacroId, S), args: MacroArgs<Token<I, L>, S>);
}

pub type VecMacroTable<I, L, H> = Vec<MacroDef<I, Token<I, L>, H>>;

pub type MacroArgs<T, S> = (Vec<Vec<T>>, Vec<Vec<S>>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroId(usize);

impl<'a, C, R: SpanSource, II: StringSource, N, B, D, I, L, H> MacroSource
    for CompositeSession<C, R, II, VecMacroTable<I, L, H>, N, B, D>
{
    type MacroId = MacroId;
}

impl<'a, C, RR, II, N, B, D, I>
    MacroTable<
        <Self as IdentSource>::Ident,
        Literal<<Self as StringSource>::StringRef>,
        <Self as SpanSource>::Span,
    >
    for CompositeSession<
        C,
        RR,
        II,
        VecMacroTable<I, Literal<II::StringRef>, <RR as AddMacroDef<RR::Span>>::MacroDefHandle>,
        N,
        B,
        D,
    >
where
    I: AsRef<str> + Debug + Clone + Eq,
    Self: Lex<RR, II, Span = RR::Span, Ident = I, StringRef = II::StringRef>,
    C: Codebase,
    RR: SpanSystem,
    II: Interner,
    Self: NextToken,
    Self: EmitDiag<RR::Span, RR::Stripped>,
    Self: StartScope<<Self as IdentSource>::Ident> + NameTable<<Self as IdentSource>::Ident>,
    Self: Backend<RR::Span>,
    Self: MacroSource<MacroId = MacroId>,
    <Self as IdentSource>::Ident: 'static,
    <Self as StringSource>::StringRef: 'static,
    <Self as SpanSource>::Span: 'static,
    <Self as Lex<RR, II>>::TokenIter: 'static,
{
    fn define_macro(
        &mut self,
        name_span: RR::Span,
        params: (Vec<<Self as IdentSource>::Ident>, Vec<RR::Span>),
        body: (
            Vec<Token<<Self as IdentSource>::Ident, Literal<<Self as StringSource>::StringRef>>>,
            Vec<RR::Span>,
        ),
    ) -> Self::MacroId {
        let context = self.registry.add_macro_def(name_span, params.1, body.1);
        let id = MacroId(self.macros.len());
        self.macros.push(MacroDef {
            tokens: Rc::new(MacroDefTokens {
                params: params.0,
                body: body.0,
            }),
            spans: context,
        });
        id
    }

    fn expand_macro(
        &mut self,
        (MacroId(id), name_span): (Self::MacroId, RR::Span),
        (args, arg_spans): MacroArgs<
            Token<<Self as IdentSource>::Ident, Literal<<Self as StringSource>::StringRef>>,
            RR::Span,
        >,
    ) {
        let def = &self.macros[id];
        let context = self
            .registry
            .mk_macro_call_ctx(name_span, arg_spans, &def.spans);
        let expansion = MacroExpansionIter::new(def.tokens.clone(), args, context);
        self.tokens.push(Box::new(expansion));
        let mut parser = <DefaultParserFactory as ParserFactory<
            Ident<String>,
            Literal<String>,
            LexError,
            RcSpan<BufId, BufRange>,
        >>::mk_parser(&mut DefaultParserFactory);
        let semantics = Semantics {
            session: self,
            state: TokenStreamState::new(),
        };
        parser.parse_token_stream(semantics);
    }
}

pub struct MacroDef<I, T, S> {
    tokens: Rc<MacroDefTokens<I, T>>,
    spans: S,
}

struct MacroDefTokens<I, T> {
    params: Vec<I>,
    body: Vec<T>,
}

pub struct MacroExpansionIter<I, T, C> {
    expansion: MacroExpansion<I, T, C>,
    pos: Option<MacroExpansionPos>,
}

struct MacroExpansion<I, T, C> {
    def: Rc<MacroDefTokens<I, T>>,
    args: Vec<Vec<T>>,
    context: C,
}

impl<I: PartialEq, L, F> MacroExpansion<I, Token<I, L>, F> {
    fn mk_macro_expansion_pos(&self, token: usize) -> Option<MacroExpansionPos> {
        if token >= self.def.body.len() {
            return None;
        }

        let param_expansion = self.def.body[token].name().and_then(|name| {
            self.param_position(name).map(|param| ParamExpansionPos {
                param,
                arg_token: 0,
            })
        });
        Some(MacroExpansionPos {
            token,
            param_expansion,
        })
    }

    fn param_position(&self, name: &I) -> Option<usize> {
        self.def.params.iter().position(|param| *param == *name)
    }

    fn next_pos(&self, pos: &MacroExpansionPos) -> Option<MacroExpansionPos> {
        let param_expansion = pos
            .param_expansion
            .as_ref()
            .and_then(|param_expansion| self.next_param_expansion_pos(&param_expansion));
        if param_expansion.is_some() {
            Some(MacroExpansionPos {
                param_expansion,
                ..*pos
            })
        } else {
            self.mk_macro_expansion_pos(pos.token + 1)
        }
    }

    fn next_param_expansion_pos(&self, pos: &ParamExpansionPos) -> Option<ParamExpansionPos> {
        if pos.arg_token + 1 < self.args[pos.param].len() {
            Some(ParamExpansionPos {
                arg_token: pos.arg_token + 1,
                ..*pos
            })
        } else {
            None
        }
    }

    fn token_and_span(&self, pos: MacroExpansionPos) -> (Token<I, L>, F::Span)
    where
        I: Clone,
        F: MacroCallCtx,
        Token<I, L>: Clone,
    {
        (self.token(&pos), self.context.mk_span(pos))
    }

    fn token(&self, pos: &MacroExpansionPos) -> Token<I, L>
    where
        I: Clone,
        Token<I, L>: Clone,
    {
        let body_token = &self.def.body[pos.token];
        pos.param_expansion.as_ref().map_or_else(
            || body_token.clone(),
            |param_expansion| match (
                body_token,
                &self.args[param_expansion.param][param_expansion.arg_token],
            ) {
                (Token::Label(_), Token::Ident(ident)) if param_expansion.arg_token == 0 => {
                    Token::Label(ident.clone())
                }
                (_, arg_token) => arg_token.clone(),
            },
        )
    }
}

impl<I, L> Token<I, L> {
    fn name(&self) -> Option<&I> {
        match &self {
            Token::Ident(name) | Token::Label(name) => Some(name),
            _ => None,
        }
    }
}

impl<I, L, F> MacroExpansionIter<I, Token<I, L>, F>
where
    I: PartialEq,
{
    fn new(
        def: Rc<MacroDefTokens<I, Token<I, L>>>,
        args: Vec<Vec<Token<I, L>>>,
        context: F,
    ) -> Self {
        let expansion = MacroExpansion { def, args, context };
        MacroExpansionIter {
            pos: expansion.mk_macro_expansion_pos(0),
            expansion,
        }
    }
}

impl<I, L, F> IdentSource for MacroExpansionIter<I, Token<I, L>, F>
where
    I: AsRef<str> + Clone + Debug + Eq,
    F: MacroCallCtx,
    Token<I, L>: Clone,
{
    type Ident = I;
}

impl<RR, II, I, R, F> TokenStream<RR, II> for MacroExpansionIter<I, Token<I, Literal<R>>, F>
where
    RR: SpanSource<Span = F::Span>,
    II: StringSource<StringRef = R>,
    I: AsRef<str> + Clone + Debug + Eq,
    R: Clone + Debug + Eq,
    F: MacroCallCtx,
    Token<I, Literal<R>>: Clone,
{
    fn next_token(
        &mut self,
        _registry: &mut RR,
        _interner: &mut II,
    ) -> Option<LexItem<Self::Ident, II::StringRef, RR::Span>> {
        self.pos.take().map(|pos| {
            self.pos = self.expansion.next_pos(&pos);
            let (token, span) = self.expansion.token_and_span(pos);
            (Ok(token), span)
        })
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;

    use crate::log::Log;
    use crate::session::lex::Literal;
    use crate::syntax::Token;

    #[derive(Debug, PartialEq)]
    pub enum MacroTableEvent {
        DefineMacro(
            Vec<Ident<String>>,
            Vec<Token<Ident<String>, Literal<String>>>,
        ),
        ExpandMacro(MockMacroId, Vec<Vec<Token<Ident<String>, Literal<String>>>>),
    }

    pub struct MockMacroTable<T> {
        log: Log<T>,
    }

    impl<T> MockMacroTable<T> {
        pub fn new(log: Log<T>) -> Self {
            Self { log }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct MockMacroId(pub usize);

    impl<T> MacroSource for MockMacroTable<T> {
        type MacroId = MockMacroId;
    }

    impl<'a, C, R: SpanSource, I: StringSource, N, B, D, T> MacroSource
        for CompositeSession<C, R, I, MockMacroTable<T>, N, B, D>
    {
        type MacroId = MockMacroId;
    }

    impl<C, R, I: StringSource, N, B, D, T> MacroTable<Ident<String>, Literal<String>, D::Span>
        for CompositeSession<C, R, I, MockMacroTable<T>, N, B, D>
    where
        R: SpanSource,
        D: SpanSource,
        T: From<MacroTableEvent>,
    {
        fn define_macro(
            &mut self,
            _name_span: D::Span,
            params: (Vec<Ident<String>>, Vec<D::Span>),
            body: (Vec<Token<Ident<String>, Literal<String>>>, Vec<D::Span>),
        ) -> Self::MacroId {
            self.macros
                .log
                .push(MacroTableEvent::DefineMacro(params.0, body.0));
            MockMacroId(0)
        }

        fn expand_macro(
            &mut self,
            name: (Self::MacroId, D::Span),
            args: MacroArgs<Token<Ident<String>, Literal<String>>, D::Span>,
        ) {
            self.macros
                .log
                .push(MacroTableEvent::ExpandMacro(name.0, args.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn expand_macro_with_one_token() {
    //     let body = Token::<_, ()>::Ident("a");
    //     let macros = vec![MacroDef {
    //         tokens: Rc::new(MacroDefTokens {
    //             params: vec![],
    //             body: vec![body.clone()],
    //         }),
    //         spans: (),
    //     }];
    //     let mut session = CompositeSession {
    //         reentrancy: SourceComponents {
    //             codebase: (),
    //             parser_factory: (),
    //             macros,
    //             interner: (),
    //         },
    //         names: (),
    //         builder: (),
    //         diagnostics: Factory,
    //     };
    //     let name = ModularSpan::Buf(());
    //     // let expanded: Vec<_> =
    //     //     MacroTable::expand_macro(&mut session, (MacroId(0), name.clone()), (vec![], vec![]))
    //     //         .collect();
    //     let data = MacroCall(Rc::new(ModularMacroCall {
    //         name,
    //         args: vec![],
    //         def: (),
    //     }));
    //     let macro_expansion_position = MacroExpansionPos {
    //         token: 0,
    //         param_expansion: None,
    //     };
    //     assert_eq!(
    //         expanded,
    //         [(
    //             body,
    //             ModularSpan::Macro(MacroSpan {
    //                 range: macro_expansion_position.clone()..=macro_expansion_position,
    //                 context: data,
    //             })
    //         )]
    //     )
    // }

    // #[test]
    // fn expand_label_using_two_idents() {
    //     let label = Token::<_, ()>::Label("label");
    //     let macros = vec![MacroDef {
    //         tokens: Rc::new(MacroDefTokens {
    //             params: vec!["label"],
    //             body: vec![label],
    //         }),
    //         spans: (),
    //     }];
    //     let mut components = SourceComponents {
    //         codebase: &mut (),
    //         parser_factory: &mut (),
    //         macros,
    //         interner: &mut (),
    //         diagnostics: &mut Factory,
    //     };
    //     let name = ModularSpan::Buf(());
    //     let arg = vec![Token::Ident("tok1"), Token::Ident("tok2")];
    //     let expanded: Vec<_> = MacroTable::expand_macro(
    //         &mut components,
    //         (MacroId(0), name.clone()),
    //         (
    //             vec![arg],
    //             vec![vec![ModularSpan::Buf(()), ModularSpan::Buf(())]],
    //         ),
    //     )
    //     .collect();
    //     let context = MacroCall(Rc::new(ModularMacroCall {
    //         name,
    //         args: vec![vec![ModularSpan::Buf(()), ModularSpan::Buf(())]],
    //         def: (),
    //     }));
    //     let tok1_pos = MacroExpansionPos {
    //         token: 0,
    //         param_expansion: Some(ParamExpansionPos {
    //             param: 0,
    //             arg_token: 0,
    //         }),
    //     };
    //     let tok2_pos = MacroExpansionPos {
    //         token: 0,
    //         param_expansion: Some(ParamExpansionPos {
    //             param: 0,
    //             arg_token: 1,
    //         }),
    //     };
    //     assert_eq!(
    //         expanded,
    //         [
    //             (
    //                 Token::Label("tok1"),
    //                 ModularSpan::Macro(MacroSpan {
    //                     range: tok1_pos.clone()..=tok1_pos,
    //                     context: context.clone()
    //                 })
    //             ),
    //             (
    //                 Token::Ident("tok2"),
    //                 ModularSpan::Macro(MacroSpan {
    //                     range: tok2_pos.clone()..=tok2_pos,
    //                     context,
    //                 })
    //             )
    //         ]
    //     )
    // }

    // #[ignore]
    // #[test]
    // fn expand_macro() {
    //     let buf = Rc::new(BufContextData {
    //         buf_id: (),
    //         included_from: None,
    //     });
    //     let mk_span = |n| {
    //         ModularSpan::Buf(BufSpan {
    //             range: n,
    //             context: Rc::clone(&buf),
    //         })
    //     };
    //     let body: Vec<Token<_, ()>> = vec![Token::Ident("a"), Token::Ident("x"), Token::Ident("b")];
    //     let def_id = Rc::new(MacroDefSpans {
    //         name: mk_span(0),
    //         params: vec![mk_span(1)],
    //         body: (2..=4).map(mk_span).collect(),
    //     });
    //     let factory = &mut RcContextFactory::new();
    //     let entry = vec![MacroDef {
    //         tokens: Rc::new(MacroDefTokens {
    //             params: vec!["x"],
    //             body,
    //         }),
    //         spans: Rc::clone(&def_id),
    //     }];
    //     let data = RcMacroCall::new(ModularMacroCall {
    //         name: ModularSpan::Buf(BufSpan {
    //             range: 7,
    //             context: buf.clone(),
    //         }),
    //         args: vec![vec![
    //             ModularSpan::Buf(BufSpan {
    //                 range: 8,
    //                 context: buf.clone(),
    //             }),
    //             ModularSpan::Buf(BufSpan {
    //                 range: 9,
    //                 context: buf.clone(),
    //             }),
    //         ]],
    //         def: def_id,
    //     });
    //     let call_name = ("my_macro", mk_span(7));
    //     let expanded: Vec<_> = entry
    //         .expand_macro(
    //             (MacroId(0), call_name.1),
    //             (
    //                 vec![vec![Token::Ident("y"), Token::Ident("z")]],
    //                 vec![(8..=9).map(mk_span).collect()],
    //             ),
    //             factory,
    //         )
    //         .collect();
    //     let mk_span_data = |token, param_expansion| {
    //         let position = MacroExpansionPos {
    //             token,
    //             param_expansion,
    //         };
    //         ModularSpan::Macro(MacroSpan {
    //             range: position.clone()..=position,
    //             context: data.clone(),
    //         })
    //     };
    //     assert_eq!(
    //         expanded,
    //         [
    //             (Token::Ident("a"), mk_span_data(0, None)),
    //             (
    //                 Token::Ident("y"),
    //                 mk_span_data(
    //                     1,
    //                     Some(ParamExpansionPos {
    //                         param: 0,
    //                         arg_token: 0
    //                     })
    //                 ),
    //             ),
    //             (
    //                 Token::Ident("z"),
    //                 mk_span_data(
    //                     1,
    //                     Some(ParamExpansionPos {
    //                         param: 0,
    //                         arg_token: 1
    //                     })
    //                 ),
    //             ),
    //             (Token::Ident("b"), mk_span_data(2, None)),
    //         ]
    //     )
    // }

    #[derive(Clone, Debug, PartialEq)]
    struct MacroCall(Rc<ModularMacroCall<(), Span>>);

    type Span = ModularSpan<(), MacroSpan<MacroCall>>;

    struct Factory;

    impl SpanSource for Factory {
        type Span = Span;
    }

    impl AddMacroDef<Span> for Factory {
        type MacroDefHandle = ();

        fn add_macro_def<P, B>(&mut self, _: Span, _: P, _: B) -> Self::MacroDefHandle
        where
            P: IntoIterator<Item = Span>,
            B: IntoIterator<Item = Span>,
        {
        }
    }

    impl MacroContextFactory<(), Span> for Factory {
        type MacroCallCtx = MacroCall;

        fn mk_macro_call_ctx<A, J>(&mut self, name: Span, args: A, _: &()) -> Self::MacroCallCtx
        where
            A: IntoIterator<Item = J>,
            J: IntoIterator<Item = Span>,
        {
            MacroCall(Rc::new(ModularMacroCall {
                name,
                args: args
                    .into_iter()
                    .map(IntoIterator::into_iter)
                    .map(Iterator::collect)
                    .collect(),
                def: (),
            }))
        }
    }

    impl SpanSource for MacroCall {
        type Span = Span;
    }

    impl MacroCallCtx for MacroCall {
        fn mk_span(&self, position: MacroExpansionPos) -> Self::Span {
            ModularSpan::Macro(MacroSpan {
                range: position.clone()..=position,
                context: self.clone(),
            })
        }
    }
}
