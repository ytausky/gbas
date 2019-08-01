use self::syntax::{ArgActions, ExprAtom, Operator, Sigil, Token, UnaryOperator};
use self::Sigil::*;

use super::{Parser, LINE_FOLLOW_SET};

use crate::analysis::syntax;
use crate::diag::span::{MergeSpans, StripSpan};
use crate::diag::{CompactDiag, EmitDiag, Message};
use crate::model::BinOp;

type ParserResult<P, C, S> = Result<P, (P, ExpandedExprParsingError<C, S>)>;
type ExpandedExprParsingError<D, S> = ExprParsingError<S, <D as StripSpan<S>>::Stripped>;

enum ExprParsingError<S, R> {
    NothingParsed,
    Other(CompactDiag<S, R>),
}

enum SuffixOperator {
    Binary(BinOp),
    FnCall,
}

impl<I, L> Token<I, L> {
    fn as_suffix_operator(&self) -> Option<SuffixOperator> {
        use SuffixOperator::*;
        match self {
            Token::Sigil(Minus) => Some(Binary(BinOp::Minus)),
            Token::Sigil(LParen) => Some(FnCall),
            Token::Sigil(Pipe) => Some(Binary(BinOp::BitwiseOr)),
            Token::Sigil(Plus) => Some(Binary(BinOp::Plus)),
            Token::Sigil(Slash) => Some(Binary(BinOp::Division)),
            Token::Sigil(Star) => Some(Binary(BinOp::Multiplication)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Precedence {
    None,
    BitwiseOr,
    Addition,
    Multiplication,
    FnCall,
}

impl SuffixOperator {
    fn precedence(&self) -> Precedence {
        use SuffixOperator::*;
        match self {
            Binary(BinOp::BitwiseOr) => Precedence::BitwiseOr,
            Binary(BinOp::Plus) | Binary(BinOp::Minus) => Precedence::Addition,
            Binary(BinOp::Multiplication) | Binary(BinOp::Division) => Precedence::Multiplication,
            FnCall => Precedence::FnCall,
        }
    }
}

impl<'a, I, L, E, R, A, S> Parser<'a, (Result<Token<I, L>, E>, S), R, A>
where
    R: Iterator<Item = (Result<Token<I, L>, E>, S)>,
    A: ArgActions<S, Ident = I, Literal = L>,
    S: Clone,
{
    pub(super) fn parse(self) -> Self {
        self.parse_expression()
            .unwrap_or_else(|(mut parser, error)| {
                match error {
                    ExprParsingError::NothingParsed => parser = parser.diagnose_unexpected_token(),
                    ExprParsingError::Other(diagnostic) => parser.emit_diag(diagnostic),
                }
                while !parser.token_is_in(LINE_FOLLOW_SET) {
                    bump!(parser);
                }
                parser
            })
    }

    fn parse_expression(self) -> ParserResult<Self, A, S> {
        self.parse_infix_expr(Precedence::None)
    }

    fn parse_parenthesized_expression(mut self, left: S) -> ParserResult<Self, A, S> {
        self = match self.parse_expression() {
            Ok(parser) => parser,
            Err((parser, error)) => {
                let error = match error {
                    error @ ExprParsingError::NothingParsed => match parser.state.token.0 {
                        Ok(Token::Sigil(Eos)) | Ok(Token::Sigil(Eol)) => {
                            ExprParsingError::Other(Message::UnmatchedParenthesis.at(left).into())
                        }
                        _ => error,
                    },
                    error => error,
                };
                return Err((parser, error));
            }
        };
        match self.state.token {
            (Ok(Token::Sigil(RParen)), right) => {
                bump!(self);
                let span = self.merge_spans(&left, &right);
                self.actions
                    .act_on_operator((Operator::Unary(UnaryOperator::Parentheses), span));
                Ok(self)
            }
            _ => Err((
                self,
                ExprParsingError::Other(Message::UnmatchedParenthesis.at(left).into()),
            )),
        }
    }

    fn parse_infix_expr(mut self, lowest: Precedence) -> ParserResult<Self, A, S> {
        self = self.parse_primary_expr()?;
        while let Some(suffix_operator) = self
            .state
            .token
            .0
            .as_ref()
            .ok()
            .map(Token::as_suffix_operator)
            .unwrap_or(None)
        {
            let precedence = suffix_operator.precedence();
            if precedence <= lowest {
                break;
            }
            let span = self.state.token.1;
            bump!(self);
            match suffix_operator {
                SuffixOperator::Binary(binary_operator) => {
                    self = self.parse_infix_expr(precedence)?;
                    self.actions
                        .act_on_operator((Operator::Binary(binary_operator), span))
                }
                SuffixOperator::FnCall => self = self.parse_fn_call(span)?,
            }
        }
        Ok(self)
    }

    fn parse_fn_call(mut self, left: S) -> ParserResult<Self, A, S> {
        let mut args = 0;
        while let Ok(token) = &self.state.token.0 {
            match token {
                Token::Sigil(Sigil::RParen) => break,
                Token::Sigil(Sigil::Comma) => {
                    bump!(self);
                    self = self.parse_fn_arg(&mut args)?;
                }
                _ => self = self.parse_fn_arg(&mut args)?,
            }
        }
        let span = self.actions.merge_spans(&left, &self.state.token.1);
        self.actions.act_on_operator((Operator::FnCall(args), span));
        bump!(self);
        Ok(self)
    }

    fn parse_fn_arg(mut self, args: &mut usize) -> ParserResult<Self, A, S> {
        self = self.parse_expression()?;
        *args += 1;
        Ok(self)
    }

    fn parse_primary_expr(mut self) -> ParserResult<Self, A, S> {
        match self.state.token {
            (Ok(Token::Sigil(LParen)), span) => {
                bump!(self);
                self.parse_parenthesized_expression(span)
            }
            _ => self.parse_atomic_expr(),
        }
    }

    fn parse_atomic_expr(mut self) -> ParserResult<Self, A, S> {
        match self.state.token.0 {
            Ok(Token::Sigil(Eos)) | Ok(Token::Sigil(Eol)) => {
                Err((self, ExprParsingError::NothingParsed))
            }
            Ok(Token::Ident(ident)) => {
                self.actions
                    .act_on_atom((ExprAtom::Ident(ident), self.state.token.1));
                bump!(self);
                Ok(self)
            }
            Ok(Token::Literal(literal)) => {
                self.actions
                    .act_on_atom((ExprAtom::Literal(literal), self.state.token.1));
                bump!(self);
                Ok(self)
            }
            Ok(Token::Sigil(Sigil::Dot)) => {
                self.actions
                    .act_on_atom((ExprAtom::LocationCounter, self.state.token.1));
                bump!(self);
                Ok(self)
            }
            _ => {
                let span = self.state.token.1;
                let stripped = self.actions.strip_span(&span);
                bump!(self);
                Err((
                    self,
                    ExprParsingError::Other(
                        Message::UnexpectedToken { token: stripped }.at(span).into(),
                    ),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use self::syntax::parser::mock::*;
    use self::syntax::ArgFinalizer;

    use super::Token::*;
    use super::*;

    use crate::diag::Merge;

    #[test]
    fn parse_long_sum_arg() {
        let tokens = input_tokens![
            w @ Ident(IdentKind::Other),
            plus1 @ Plus,
            x @ Ident(IdentKind::Other),
            plus2 @ Plus,
            y @ Literal(()),
            plus3 @ Plus,
            z @ Ident(IdentKind::Other),
        ];
        let expected = expr()
            .ident("w")
            .ident("x")
            .plus("plus1")
            .literal("y")
            .plus("plus2")
            .ident("z")
            .plus("plus3");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_subtraction() {
        let tokens = input_tokens![x @ Ident(IdentKind::Other), minus @ Minus, y @ Literal(())];
        let expected = expr().ident("x").literal("y").minus("minus");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_division() {
        let tokens = input_tokens![x @ Ident(IdentKind::Other), slash @ Slash, y @ Literal(())];
        let expected = expr().ident("x").literal("y").divide("slash");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn multiplication_precedes_addition() {
        let tokens = input_tokens![
            a @ Literal(()),
            plus @ Plus,
            b @ Literal(()),
            star @ Star,
            c @ Literal(()),
        ];
        let expected = expr()
            .literal("a")
            .literal("b")
            .literal("c")
            .multiply("star")
            .plus("plus");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_sum_of_terms() {
        let tokens = input_tokens![
            a @ Literal(()),
            slash @ Slash,
            b @ Literal(()),
            plus @ Plus,
            c @ Literal(()),
            star @ Star,
            d @ Ident(IdentKind::Other),
        ];
        let expected = expr()
            .literal("a")
            .literal("b")
            .divide("slash")
            .literal("c")
            .ident("d")
            .multiply("star")
            .plus("plus");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_multiplication() {
        let tokens = input_tokens![x @ Ident(IdentKind::Other), star @ Star, y @ Literal(())];
        let expected = expr().ident("x").literal("y").multiply("star");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_bitwise_or() {
        let tokens = input_tokens![x @ Ident(IdentKind::Other), pipe @ Pipe, y @ Literal(())];
        let expected = expr().ident("x").literal("y").bitwise_or("pipe");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn addition_precedes_bitwise_or() {
        let tokens = input_tokens![
            x @ Ident(IdentKind::Other),
            pipe @ Pipe,
            y @ Ident(IdentKind::Other),
            plus @ Plus,
            z @ Ident(IdentKind::Other),
        ];
        let expected = expr()
            .ident("x")
            .ident("y")
            .ident("z")
            .plus("plus")
            .bitwise_or("pipe");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_nullary_fn_call() {
        let tokens = input_tokens![name @ Ident(IdentKind::Other), left @ LParen, right @ RParen];
        let expected = expr().ident("name").fn_call(
            0,
            MockSpan::merge(TokenRef::from("left"), TokenRef::from("right")),
        );
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_unary_fn_call() {
        let tokens = input_tokens![
            name @ Ident(IdentKind::Other),
            left @ LParen,
            arg @ Ident(IdentKind::Other),
            right @ RParen
        ];
        let expected = expr().ident("name").ident("arg").fn_call(
            1,
            MockSpan::merge(TokenRef::from("left"), TokenRef::from("right")),
        );
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_binary_fn_call() {
        let tokens = input_tokens![
            name @ Ident(IdentKind::Other),
            left @ LParen,
            arg1 @ Ident(IdentKind::Other),
            Sigil(Comma),
            arg2 @ Ident(IdentKind::Other),
            right @ RParen
        ];
        let expected = expr().ident("name").ident("arg1").ident("arg2").fn_call(
            2,
            MockSpan::merge(TokenRef::from("left"), TokenRef::from("right")),
        );
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_fn_call_plus_literal() {
        let tokens = input_tokens![
            name @ Ident(IdentKind::Other),
            left @ LParen,
            right @ RParen,
            plus @ Sigil(Plus),
            literal @ Literal(())
        ];
        let expected = expr()
            .ident("name")
            .fn_call(
                0,
                MockSpan::merge(TokenRef::from("left"), TokenRef::from("right")),
            )
            .literal("literal")
            .plus("plus");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn fn_call_precedes_multiplication() {
        let tokens = input_tokens![
            literal @ Literal(()),
            star @ Sigil(Star),
            name @ Ident(IdentKind::Other),
            left @ LParen,
            right @ RParen,
        ];
        let expected = expr()
            .literal("literal")
            .ident("name")
            .fn_call(
                0,
                MockSpan::merge(TokenRef::from("left"), TokenRef::from("right")),
            )
            .multiply("star");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_location_counter() {
        let tokens = input_tokens![dot @ Sigil(Dot)];
        let expected = expr().location_counter("dot");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn parse_sum_with_parentheses() {
        let tokens = input_tokens![
            a @ Ident(IdentKind::Other),
            plus1 @ Plus,
            left @ LParen,
            b @ Ident(IdentKind::Other),
            plus2 @ Plus,
            c @ Ident(IdentKind::Other),
            right @ RParen,
        ];
        let expected = expr()
            .ident("a")
            .ident("b")
            .ident("c")
            .plus("plus2")
            .parentheses("left", "right")
            .plus("plus1");
        assert_eq_rpn_expr(tokens, expected)
    }

    #[test]
    fn diagnose_eos_for_rhs_operand() {
        assert_eq_rpn_expr(
            input_tokens![Ident(IdentKind::Other), Plus],
            expr()
                .ident(0)
                .error(Message::UnexpectedEof, TokenRef::from(2)),
        )
    }

    #[test]
    fn diagnose_unexpected_token_in_expr() {
        let input = input_tokens![plus @ Plus];
        let span: MockSpan = TokenRef::from("plus").into();
        assert_eq_expr_diagnostics(
            input,
            Message::UnexpectedToken {
                token: span.clone(),
            }
            .at(span)
            .into(),
        )
    }

    fn assert_eq_rpn_expr(mut input: InputTokens, SymExpr(expected): SymExpr) {
        let parsed_rpn_expr = parse_sym_expr(&mut input);
        assert_eq!(parsed_rpn_expr, expected);
    }

    fn assert_eq_expr_diagnostics(mut input: InputTokens, expected: CompactDiag<MockSpan>) {
        let expr_actions = parse_sym_expr(&mut input);
        assert_eq!(expr_actions, [ExprAction::EmitDiag(expected)])
    }

    fn parse_sym_expr(input: &mut InputTokens) -> Vec<ExprAction<MockSpan>> {
        let tokens = &mut with_spans(&input.tokens);
        Parser::new(tokens, ExprActionCollector::new(()))
            .parse()
            .change_context(ArgFinalizer::did_parse_arg)
            .actions
    }
}
