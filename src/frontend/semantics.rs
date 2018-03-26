use frontend::ast;
use frontend::syntax;

use frontend::ast::Expression;
use frontend::syntax::{Keyword, Token};

use ir::*;

pub struct AstBuilder<'a, S: Section> {
    ast: Vec<ast::AsmItem<'a>>,
    contexts: Vec<Context<'a>>,
    section: S,
}

enum Context<'a> {
    Block,
    Instruction(Token<'a>, Vec<ast::Expression<Token<'a>>>),
}

impl<'a, S: Section> AstBuilder<'a, S> {
    pub fn new(section: S) -> AstBuilder<'a, S> {
        AstBuilder {
            ast: Vec::new(),
            contexts: vec![Context::Block],
            section: section,
        }
    }

    #[cfg(test)]
    fn ast(&self) -> &Vec<ast::AsmItem<'a>> {
        &self.ast
    }
}

impl<'a, S: Section> syntax::BlockContext for AstBuilder<'a, S> {
    type Terminal = Token<'a>;
    type Expr = Expression<Self::Terminal>;
    type CommandContext = Self;
    type TerminalSequenceContext = Self;

    fn add_label(&mut self, label: Self::Terminal) {
        match label {
            Token::Label(spelling) => self.section.add_label(spelling),
            _ => panic!(),
        }
    }

    fn enter_command(&mut self, name: Self::Terminal) -> &mut Self::CommandContext {
        self.contexts.push(Context::Instruction(name, vec![]));
        self
    }

    fn enter_macro_definition(
        &mut self,
        _label: Self::Terminal,
    ) -> &mut Self::TerminalSequenceContext {
        unimplemented!()
    }
}

impl<'a, S: Section> syntax::CommandContext for AstBuilder<'a, S> {
    type Terminal = Token<'a>;
    type Expr = Expression<Self::Terminal>;

    fn add_argument(&mut self, expr: Self::Expr) {
        match self.contexts.last_mut() {
            Some(&mut Context::Instruction(_, ref mut args)) => args.push(expr),
            _ => panic!(),
        }
    }

    fn exit_command(&mut self) {
        if let Some(Context::Instruction(name, args)) = self.contexts.pop() {
            match name {
                Token::Keyword(Keyword::Include) => self.ast.push(reduce_include(args)),
                Token::Keyword(keyword) => self.section
                    .add_instruction(reduce_mnemonic(keyword, args.into_iter())),
                _ => panic!(),
            }
        } else {
            panic!()
        }
    }
}

impl<'a, S: Section> syntax::TerminalSequenceContext for AstBuilder<'a, S> {
    type Terminal = Token<'a>;

    fn push_terminal(&mut self, _terminal: Self::Terminal) {
        unimplemented!()
    }

    fn exit_terminal_sequence(&mut self) {
        unimplemented!()
    }
}

fn reduce_include<'a>(mut arguments: Vec<Expression<Token<'a>>>) -> ast::AsmItem<'a> {
    assert_eq!(arguments.len(), 1);
    let path = arguments.pop().unwrap();
    match path {
        Expression::Atom(Token::QuotedString(path_str)) => include(path_str),
        _ => panic!(),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Operand {
    Alu(AluOperand),
    Reg16(Reg16),
}

fn reduce_mnemonic<'a, I>(command: Keyword, operands: I) -> Instruction
where
    I: Iterator<Item = Expression<Token<'a>>>,
{
    instruction(to_mnemonic(command), operands.map(interpret_as_operand))
}

fn interpret_as_operand<'a>(expr: Expression<Token<'a>>) -> Operand {
    match expr {
        Expression::Atom(Token::Keyword(keyword)) => interpret_as_keyword_operand(keyword),
        Expression::Deref(address_specifier) => interpret_as_deref_operand(*address_specifier),
        _ => panic!(),
    }
}

fn interpret_as_keyword_operand(keyword: Keyword) -> Operand {
    match keyword {
        Keyword::A => Operand::Alu(AluOperand::A),
        Keyword::B => Operand::Alu(AluOperand::B),
        Keyword::Bc => Operand::Reg16(Reg16::Bc),
        _ => panic!(),
    }
}

fn interpret_as_deref_operand<'a>(addr: Expression<Token<'a>>) -> Operand {
    match addr {
        Expression::Atom(Token::Keyword(Keyword::Hl)) => Operand::Alu(AluOperand::DerefHl),
        _ => panic!(),
    }
}

#[derive(Debug, PartialEq)]
enum Mnemonic {
    Halt,
    Ld,
    Nop,
    Push,
    Stop,
    Xor,
}

fn to_mnemonic(keyword: Keyword) -> Mnemonic {
    match keyword {
        Keyword::Halt => Mnemonic::Halt,
        Keyword::Ld => Mnemonic::Ld,
        Keyword::Nop => Mnemonic::Nop,
        Keyword::Push => Mnemonic::Push,
        Keyword::Stop => Mnemonic::Stop,
        Keyword::Xor => Mnemonic::Xor,
        _ => panic!(),
    }
}

fn instruction<I>(mnemonic: Mnemonic, mut operands: I) -> Instruction
where
    I: Iterator<Item = Operand>,
{
    use self::Mnemonic::*;
    match mnemonic {
        Halt => Instruction::Halt,
        Ld => analyze_ld(operands),
        Nop => Instruction::Nop,
        Push => match operands.next() {
            Some(Operand::Reg16(src)) => Instruction::Push(src),
            _ => panic!(),
        },
        Stop => Instruction::Stop,
        Xor => match operands.next() {
            Some(Operand::Alu(src)) => Instruction::Xor(src),
            _ => panic!(),
        },
    }
}

fn analyze_ld<I: Iterator<Item = Operand>>(mut operands: I) -> Instruction {
    let dest = operands.next().unwrap();
    let src = operands.next().unwrap();
    assert_eq!(operands.next(), None);
    match (dest, src) {
        (Operand::Alu(dest), Operand::Alu(src)) => Instruction::LdAluAlu(dest, src),
        _ => panic!(),
    }
}

fn include(path: &str) -> ast::AsmItem {
    ast::AsmItem::Include(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use self::ast::ExprFactory;
    use self::syntax::*;

    #[test]
    fn build_include_item() {
        let filename = "file.asm";
        let (_, mut items) = analyze_command(Keyword::Include, &[Token::QuotedString(filename)]);
        let item = items.pop().unwrap();
        assert_eq!(item, include(filename))
    }

    #[test]
    fn parse_nop() {
        analyze_nullary_instruction(Keyword::Nop, Mnemonic::Nop)
    }

    #[test]
    fn parse_halt() {
        analyze_nullary_instruction(Keyword::Halt, Mnemonic::Halt)
    }

    #[test]
    fn parse_stop() {
        analyze_nullary_instruction(Keyword::Stop, Mnemonic::Stop)
    }

    #[test]
    fn analyze_push_bc() {
        let item = analyze_instruction(Keyword::Push, &[Token::Keyword(Keyword::Bc)]);
        assert_eq!(item, inst(Mnemonic::Push, &[BC]))
    }

    const BC: Operand = Operand::Reg16(Reg16::Bc);

    #[test]
    fn analyze_ld_a_a() {
        let token_a = Token::Keyword(Keyword::A);
        let item = analyze_instruction(Keyword::Ld, &[token_a.clone(), token_a]);
        assert_eq!(item, inst(Mnemonic::Ld, &[A, A]))
    }

    const A: Operand = Operand::Alu(AluOperand::A);

    #[test]
    fn analyze_ld_a_b() {
        let token_a = Token::Keyword(Keyword::A);
        let token_b = Token::Keyword(Keyword::B);
        let item = analyze_instruction(Keyword::Ld, &[token_a, token_b]);
        assert_eq!(item, inst(Mnemonic::Ld, &[A, B]))
    }

    const B: Operand = Operand::Alu(AluOperand::B);

    #[test]
    fn analyze_xor_a() {
        let actions = analyze_instruction(Keyword::Xor, &[Token::Keyword(Keyword::A)]);
        assert_eq!(actions, inst(Mnemonic::Xor, &[A]))
    }

    #[test]
    fn analyze_xor_deref_hl() {
        let mut actions = Vec::new();
        {
            let mut builder = AstBuilder::new(TestSection::new(&mut actions));
            let command = builder.enter_command(Token::Keyword(Keyword::Xor));
            let mut expr_builder = ast::ExprBuilder::new();
            let atom = expr_builder.from_atom(Token::Keyword(Keyword::Hl));
            let expr = expr_builder.apply_deref(atom);
            command.add_argument(expr);
            command.exit_command()
        }
        assert_eq!(
            actions,
            inst(Mnemonic::Xor, &[Operand::Alu(AluOperand::DerefHl)])
        )
    }

    fn analyze_nullary_instruction(keyword: Keyword, mnemonic: Mnemonic) {
        let item = analyze_instruction(keyword, &[]);
        assert_eq!(item, inst(mnemonic, &[]))
    }

    fn inst(mnemonic: Mnemonic, operands: &[Operand]) -> TestActions {
        vec![
            Action::AddInstruction(instruction(mnemonic, operands.iter().cloned())),
        ]
    }

    fn analyze_instruction<'a>(keyword: Keyword, operands: &[Token<'a>]) -> TestActions {
        analyze_command(keyword, operands).0
    }

    fn analyze_command<'a>(
        keyword: Keyword,
        operands: &[Token<'a>],
    ) -> (TestActions, Vec<ast::AsmItem<'a>>) {
        let mut instructions = Vec::new();
        let ast;
        {
            let mut builder = AstBuilder::new(TestSection::new(&mut instructions));
            builder.enter_command(Token::Keyword(keyword));
            for arg in operands {
                let mut expr_builder = ast::ExprBuilder::new();
                let expr = expr_builder.from_atom(arg.clone());
                builder.add_argument(expr);
            }
            builder.exit_command();
            ast = builder.ast().to_vec();
        }
        (instructions, ast)
    }

    type TestActions = Vec<Action>;

    #[derive(Debug, PartialEq)]
    enum Action {
        AddLabel(String),
        AddInstruction(Instruction),
    }

    struct TestSection<'a> {
        actions: &'a mut TestActions,
    }

    impl<'a> TestSection<'a> {
        fn new(actions: &'a mut TestActions) -> TestSection<'a> {
            TestSection { actions: actions }
        }
    }

    impl<'a> Section for TestSection<'a> {
        fn add_instruction(&mut self, instruction: Instruction) {
            self.actions.push(Action::AddInstruction(instruction))
        }

        fn add_label(&mut self, label: &str) {
            self.actions.push(Action::AddLabel(label.to_string()))
        }
    }

    #[test]
    fn analyze_label() {
        let mut actions = Vec::new();
        {
            let mut builder = AstBuilder::new(TestSection::new(&mut actions));
            builder.add_label(Token::Label("label"));
        }
        assert_eq!(actions, vec![Action::AddLabel("label".to_string())])
    }
}