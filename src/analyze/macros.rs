use crate::analyze::Token;
use crate::diag::span::*;

use std::rc::Rc;

pub(super) trait Expand<T, H, F: MacroContextFactory<H, S> + ?Sized, S: Clone> {
    type Iter: Iterator<Item = (T, S)>;

    fn expand(&self, name: S, args: MacroArgs<T, S>, factory: &mut F) -> Self::Iter;
}

pub(super) type MacroTable<I, L, H> = Vec<MacroDef<I, Token<I, L>, H>>;

pub(super) type MacroArgs<T, S> = (Vec<Vec<T>>, Vec<Vec<S>>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroId(pub(super) usize);

pub(super) trait DefineMacro<I, T, H: Clone> {
    fn define_macro<D, S>(
        &mut self,
        name_span: S,
        params: (Vec<I>, Vec<S>),
        body: (Vec<T>, Vec<S>),
        diagnostics: &mut D,
    ) -> MacroId
    where
        D: AddMacroDef<S, MacroDefHandle = H> + MacroContextFactory<H, S> + ?Sized,
        S: Clone;
}

impl<I, L, H: Clone> DefineMacro<I, Token<I, L>, H> for MacroTable<I, L, H> {
    fn define_macro<D, S>(
        &mut self,
        name_span: S,
        params: (Vec<I>, Vec<S>),
        body: (Vec<Token<I, L>>, Vec<S>),
        diagnostics: &mut D,
    ) -> MacroId
    where
        D: AddMacroDef<S, MacroDefHandle = H> + MacroContextFactory<H, S> + ?Sized,
        S: Clone,
    {
        let context = diagnostics.add_macro_def(name_span, params.1, body.1);
        let id = MacroId(self.len());
        self.push(MacroDef {
            tokens: Rc::new(MacroDefTokens {
                params: params.0,
                body: body.0,
            }),
            spans: context,
        });
        id
    }
}

pub(in crate::analyze) struct MacroDef<I, T, S> {
    tokens: Rc<MacroDefTokens<I, T>>,
    spans: S,
}

struct MacroDefTokens<I, T> {
    params: Vec<I>,
    body: Vec<T>,
}

impl<I, L, H, F, S> Expand<Token<I, L>, H, F, S> for MacroDef<I, Token<I, L>, H>
where
    I: Clone + PartialEq,
    F: MacroContextFactory<H, S> + ?Sized,
    S: Clone,
    Token<I, L>: Clone,
{
    type Iter = MacroExpansionIter<I, Token<I, L>, F::MacroCallCtx>;

    fn expand(
        &self,
        name: S,
        (args, arg_spans): MacroArgs<Token<I, L>, S>,
        factory: &mut F,
    ) -> Self::Iter {
        let context = factory.mk_macro_call_ctx(name, arg_spans, &self.spans);
        MacroExpansionIter::new(self.tokens.clone(), args, context)
    }
}

pub(super) struct MacroExpansionIter<I, T, C> {
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

impl<I, L, F> Iterator for MacroExpansionIter<I, Token<I, L>, F>
where
    I: Clone + PartialEq,
    F: MacroCallCtx,
    Token<I, L>: Clone,
{
    type Item = (Token<I, L>, F::Span);

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.take().map(|pos| {
            self.pos = self.expansion.next_pos(&pos);
            self.expansion.token_and_span(pos)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_macro_with_one_token() {
        let body = Token::<_, ()>::Ident("a");
        let entry = MacroDef {
            tokens: Rc::new(MacroDefTokens {
                params: vec![],
                body: vec![body.clone()],
            }),
            spans: (),
        };
        let name = ModularSpan::Buf(());
        let expanded: Vec<_> = entry
            .expand(name.clone(), (vec![], vec![]), &mut Factory)
            .collect();
        let data = MacroCall(Rc::new(ModularMacroCall {
            name,
            args: vec![],
            def: (),
        }));
        let macro_expansion_position = MacroExpansionPos {
            token: 0,
            param_expansion: None,
        };
        assert_eq!(
            expanded,
            [(
                body,
                ModularSpan::Macro(MacroSpan {
                    range: macro_expansion_position.clone()..=macro_expansion_position,
                    context: data,
                })
            )]
        )
    }

    #[test]
    fn expand_label_using_two_idents() {
        let label = Token::<_, ()>::Label("label");
        let def = MacroDef {
            tokens: Rc::new(MacroDefTokens {
                params: vec!["label"],
                body: vec![label],
            }),
            spans: (),
        };
        let name = ModularSpan::Buf(());
        let arg = vec![Token::Ident("tok1"), Token::Ident("tok2")];
        let expanded: Vec<_> = def
            .expand(
                name.clone(),
                (
                    vec![arg],
                    vec![vec![ModularSpan::Buf(()), ModularSpan::Buf(())]],
                ),
                &mut Factory,
            )
            .collect();
        let context = MacroCall(Rc::new(ModularMacroCall {
            name,
            args: vec![vec![ModularSpan::Buf(()), ModularSpan::Buf(())]],
            def: (),
        }));
        let tok1_pos = MacroExpansionPos {
            token: 0,
            param_expansion: Some(ParamExpansionPos {
                param: 0,
                arg_token: 0,
            }),
        };
        let tok2_pos = MacroExpansionPos {
            token: 0,
            param_expansion: Some(ParamExpansionPos {
                param: 0,
                arg_token: 1,
            }),
        };
        assert_eq!(
            expanded,
            [
                (
                    Token::Label("tok1"),
                    ModularSpan::Macro(MacroSpan {
                        range: tok1_pos.clone()..=tok1_pos,
                        context: context.clone()
                    })
                ),
                (
                    Token::Ident("tok2"),
                    ModularSpan::Macro(MacroSpan {
                        range: tok2_pos.clone()..=tok2_pos,
                        context,
                    })
                )
            ]
        )
    }

    #[ignore]
    #[test]
    fn expand_macro() {
        let buf = Rc::new(BufContextData {
            buf_id: (),
            included_from: None,
        });
        let mk_span = |n| {
            ModularSpan::Buf(BufSpan {
                range: n,
                context: Rc::clone(&buf),
            })
        };
        let body: Vec<Token<_, ()>> = vec![Token::Ident("a"), Token::Ident("x"), Token::Ident("b")];
        let def_id = Rc::new(MacroDefSpans {
            name: mk_span(0),
            params: vec![mk_span(1)],
            body: (2..=4).map(mk_span).collect(),
        });
        let factory = &mut RcContextFactory::new();
        let entry = MacroDef {
            tokens: Rc::new(MacroDefTokens {
                params: vec!["x"],
                body,
            }),
            spans: Rc::clone(&def_id),
        };
        let data = RcMacroCall::new(ModularMacroCall {
            name: ModularSpan::Buf(BufSpan {
                range: 7,
                context: buf.clone(),
            }),
            args: vec![vec![
                ModularSpan::Buf(BufSpan {
                    range: 8,
                    context: buf.clone(),
                }),
                ModularSpan::Buf(BufSpan {
                    range: 9,
                    context: buf.clone(),
                }),
            ]],
            def: def_id,
        });
        let call_name = ("my_macro", mk_span(7));
        let expanded: Vec<_> = entry
            .expand(
                call_name.1,
                (
                    vec![vec![Token::Ident("y"), Token::Ident("z")]],
                    vec![(8..=9).map(mk_span).collect()],
                ),
                factory,
            )
            .collect();
        let mk_span_data = |token, param_expansion| {
            let position = MacroExpansionPos {
                token,
                param_expansion,
            };
            ModularSpan::Macro(MacroSpan {
                range: position.clone()..=position,
                context: data.clone(),
            })
        };
        assert_eq!(
            expanded,
            [
                (Token::Ident("a"), mk_span_data(0, None)),
                (
                    Token::Ident("y"),
                    mk_span_data(
                        1,
                        Some(ParamExpansionPos {
                            param: 0,
                            arg_token: 0
                        })
                    ),
                ),
                (
                    Token::Ident("z"),
                    mk_span_data(
                        1,
                        Some(ParamExpansionPos {
                            param: 0,
                            arg_token: 1
                        })
                    ),
                ),
                (Token::Ident("b"), mk_span_data(2, None)),
            ]
        )
    }

    #[derive(Clone, Debug, PartialEq)]
    struct MacroCall(Rc<ModularMacroCall<(), Span>>);

    type Span = ModularSpan<(), MacroSpan<MacroCall>>;

    struct Factory;

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