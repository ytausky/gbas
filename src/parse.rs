use syntax::*;
use syntax::TerminalKind::*;

use std::iter;
use std::marker::PhantomData;

pub fn parse_src<'a, I, B>(tokens: I, block_context: &mut B)
    where I: Iterator<Item = B::Terminal>, B: BlockContext
{
    let mut parser = Parser {
        tokens: tokens.peekable(),
        _phantom: PhantomData,
    };
    parser.parse_block(block_context)
}

struct Parser<I: Iterator, B: BlockContext> {
    tokens: iter::Peekable<I>,
    _phantom: PhantomData<B>,
}

impl<I, B> Parser<I, B> where B: BlockContext, I: Iterator<Item = B::Terminal> {
    fn parse_block(&mut self, block_context: &mut B) {
        while let Some(token) = self.next_token_if_not_block_delimiter() {
            self.parse_line(token, block_context)
        }
    }

    fn next_token_if_not_block_delimiter(&mut self) -> Option<I::Item> {
        let take_next = match self.tokens.peek() {
            Some(token) if token.kind() != Endm => true,
            _ => false,
        };
        if take_next {
            self.tokens.next()
        } else {
            None
        }
    }

    fn parse_line(&mut self, first_token: I::Item, block_context: &mut B) {
        if first_token.kind() != Eol {
            self.parse_nonempty_line(first_token, block_context)
        }
    }

    fn parse_nonempty_line(&mut self, first_token: I::Item, block_context: &mut B) {
        if first_token.kind() == Label {
            self.parse_macro_definition(first_token, block_context)
        } else {
            let instruction_context = block_context.enter_instruction(first_token);
            self.parse_operands(instruction_context);
            instruction_context.exit_instruction()
        }
    }

    fn parse_macro_definition(&mut self, label: I::Item, block_context: &mut B) {
        let macro_block_context = block_context.enter_macro_definition(label);
        assert_eq!(self.tokens.next().unwrap().kind(), Colon);
        assert_eq!(self.tokens.next().unwrap().kind(), Macro);
        assert_eq!(self.tokens.next().unwrap().kind(), Eol);
        self.parse_block(macro_block_context);
        assert_eq!(self.tokens.next().unwrap().kind(), Endm);
        macro_block_context.exit_block()
    }

    fn parse_operands(&mut self, instruction_context: &mut B::InstructionContext) {
        if let Some(_) = self.peek_not_eol() {
            self.parse_expression(instruction_context);
            while let Some(Comma) = self.tokens.peek().map(|t| t.kind()) {
                self.tokens.next();
                self.parse_expression(instruction_context)
            }
        }
    }

    fn peek_not_eol(&mut self) -> Option<&I::Item> {
        match self.tokens.peek() {
            Some(token) if token.kind() == Eol => None,
            option_token => option_token,
        }
    }

    fn parse_expression(&mut self, instruction_context: &mut B::InstructionContext) {
        let expression_context = instruction_context.enter_argument();
        let token = self.tokens.next().unwrap();
        expression_context.push_atom(token);
        expression_context.exit_expression()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_src;

    use syntax;
    use syntax::TerminalKind::*;

    #[test]
    fn parse_empty_src() {
        assert_eq_actions(&[], &[])
    }

    struct TestContext {
        actions: Vec<Action>,
    }

    impl TestContext {
        fn new() -> TestContext {
            TestContext {
                actions: Vec::new(),
            }
        }
    }

    #[derive(Debug, PartialEq)]
    enum Action {
        EnterExpression,
        EnterInstruction(TestToken),
        EnterMacroDef(TestToken),
        ExitExpression,
        ExitInstruction,
        ExitMacroDef,
        PushAtom(TestToken),
    }

    type TestToken = (syntax::TerminalKind, usize);

    impl syntax::Terminal for TestToken {
        fn kind(&self) -> syntax::TerminalKind {
            let (ref terminal_kind, _) = *self;
            terminal_kind.clone()
        }
    }

    impl syntax::BlockContext for TestContext {
        type Terminal = TestToken;
        type InstructionContext = Self;

        fn enter_instruction(&mut self, name: Self::Terminal) -> &mut Self::InstructionContext {
            self.actions.push(Action::EnterInstruction(name));
            self
        }

        fn enter_macro_definition(&mut self, label: Self::Terminal) -> &mut Self {
            self.actions.push(Action::EnterMacroDef(label));
            self
        }

        fn exit_block(&mut self) {
            self.actions.push(Action::ExitMacroDef)
        }
    }

    impl syntax::InstructionContext for TestContext {
        type Terminal = TestToken;
        type ExpressionContext = Self;

        fn enter_argument(&mut self) -> &mut Self::ExpressionContext {
            self.actions.push(Action::EnterExpression);
            self
        }

        fn exit_instruction(&mut self) {
            self.actions.push(Action::ExitInstruction)
        }
    }

    impl syntax::ExpressionContext for TestContext {
        type Terminal = TestToken;

        fn push_atom(&mut self, atom: Self::Terminal) {
            self.actions.push(Action::PushAtom(atom))
        }

        fn exit_expression(&mut self) {
            self.actions.push(Action::ExitExpression)
        }
    }

    #[test]
    fn parse_empty_line() {
        assert_eq_actions(&[(Eol, 0)], &[])
    }

    fn assert_eq_actions(tokens: &[TestToken], expected_actions: &[Action]) {
        let mut parsing_constext = TestContext::new();
        parse_src(tokens.iter().cloned(), &mut parsing_constext);
        assert_eq!(parsing_constext.actions, expected_actions)
    }

    #[test]
    fn parse_nullary_instruction() {
        assert_eq_actions(&[(Word, 0)], &inst((Word, 0), vec![]))
    }

    fn inst(name: TestToken, args: Vec<Vec<Action>>) -> Vec<Action> {
        let mut result = vec![Action::EnterInstruction(name)];
        for mut arg in args {
            result.append(&mut arg);
        }
        result.push(Action::ExitInstruction);
        result
    }

    #[test]
    fn parse_nullary_instruction_followed_by_eol() {
        assert_eq_actions(&[(Word, 0), (Eol, 1)], &inst((Word, 0), vec![]))
    }

    #[test]
    fn parse_unary_instruction() {
        assert_eq_actions(&[(Word, 0), (Word, 1)], &inst((Word, 0), vec![expr(ident((Word, 1)))]))
    }

    fn expr(mut actions: Vec<Action>) -> Vec<Action> {
        let mut result = vec![Action::EnterExpression];
        result.append(&mut actions);
        result.push(Action::ExitExpression);
        result
    }

    fn ident(identifier: TestToken) -> Vec<Action> {
        vec![Action::PushAtom(identifier)]
    }

    #[test]
    fn parse_binary_instruction() {
        assert_eq_actions(&[(Word, 0), (Word, 1), (Comma, 2), (Word, 3)],
                          &inst((Word, 0), vec![expr(ident((Word, 1))), expr(ident((Word, 3)))]));
    }

    #[test]
    fn parse_two_instructions() {
        let tokens = &[
            (Word, 0), (Word, 1), (Comma, 2), (Word, 3), (Eol, 4),
            (Word, 5), (Word, 6), (Comma, 7), (Word, 8),
        ];
        let expected_actions = &concat(vec![
            inst((Word, 0), vec![
                expr(ident((Word, 1))),
                expr(ident((Word, 3))),
            ]),
            inst((Word, 5), vec![
                expr(ident((Word, 6))),
                expr(ident((Word, 8))),
            ]),
        ]);
        assert_eq_actions(tokens, expected_actions)
    }

    fn concat(actions: Vec<Vec<Action>>) -> Vec<Action> {
        let mut result = Vec::new();
        for mut vector in actions {
            result.append(&mut vector)
        }
        result
    }

    #[test]
    fn parse_two_instructions_separated_by_blank_line() {
        let tokens = &[
            (Word, 0), (Word, 1), (Comma, 2), (Word, 3), (Eol, 4),
            (Eol, 5),
            (Word, 6), (Word, 7), (Comma, 8), (Word, 9),
        ];
        let expected_actions = &concat(vec![
            inst((Word, 0), vec![
                expr(ident((Word, 1))),
                expr(ident((Word, 3))),
            ]),
            inst((Word, 6), vec![
                expr(ident((Word, 7))),
                expr(ident((Word, 9))),
            ]),
        ]);
        assert_eq_actions(tokens, expected_actions)
    }

    #[test]
    fn parse_include() {
        let tokens = &[(Word, 0), (QuotedString, 1)];
        let expected_actions = &inst((Word, 0), vec![expr(ident((QuotedString, 1)))]);
        assert_eq_actions(tokens, expected_actions);
    }

    #[test]
    fn parse_empty_macro_definition() {
        let tokens = &[
            (Label, 0), (Colon, 1), (Macro, 2), (Eol, 3),
            (Endm, 4),
        ];
        let expected_actions = &macro_def((Label, 0), vec![]);
        assert_eq_actions(tokens, expected_actions);
    }

    fn macro_def(label: TestToken, mut instructions: Vec<Action>) -> Vec<Action> {
        let mut result = vec![Action::EnterMacroDef(label)];
        result.append(&mut instructions);
        result.push(Action::ExitMacroDef);
        result
    }

    #[test]
    fn parse_macro_definition_with_instruction() {
        let tokens = &[
            (Label, 0), (Colon, 1), (Macro, 2), (Eol, 3),
            (Word, 4), (Eol, 5),
            (Endm, 6),
        ];
        let expected_actions = &macro_def((Label, 0), inst((Word, 4), vec![]));
        assert_eq_actions(tokens, expected_actions);
    }
}
