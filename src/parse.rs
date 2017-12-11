#[cfg(test)]
use ast;

#[cfg(test)]
use std::str;

#[cfg(test)]
fn parse_src(src: &str) -> Parser {
    Parser {
        src: src.lines(),
    }
}

#[cfg(test)]
struct Parser<'a> {
    src: str::Lines<'a>,
}

#[cfg(test)]
impl<'a> Iterator for Parser<'a> {
    type Item = ast::Instruction;

    fn next(&mut self) -> Option<ast::Instruction> {
        let mut parsed_line = None;
        while parsed_line == None {
            parsed_line = parse_line(self.src.next()?)
        };
        parsed_line
    }
}

#[cfg(test)]
fn parse_line(line: &str) -> Option<ast::Instruction> {
    let trimmed_line = line.trim();
    if let Some(first_space) = trimmed_line.find(' ') {
        let (mnemonic, operands) = trimmed_line.split_at(first_space);
        Some(ast::Instruction::new(mnemonic, &parse_operands(operands)))
    } else {
        match trimmed_line {
            "nop" | "halt" | "stop" => Some(ast::Instruction::new(trimmed_line, &[])),
            _ => None
        }
    }
}

#[cfg(test)]
fn parse_operands(src: &str) -> Vec<ast::Operand> {
    src.split(',').map(|op| parse_operand(op).unwrap()).collect()
}

#[cfg(test)]
fn parse_operand(src: &str) -> Option<ast::Operand> {
    match src.trim() {
        "a" => Some(ast::Operand::Register(ast::Register::A)),
        "b" => Some(ast::Operand::Register(ast::Register::B)),
        "bc" => Some(ast::Operand::RegisterPair(ast::RegisterPair::Bc)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ast::*;

    fn inst(mnemonic: &str, operands: &[Operand]) -> Instruction {
        Instruction::new(mnemonic, operands)
    }

    fn assert_ast_eq(src: &str, expected_ast: &[Instruction]) {
        let actual = parse_src(src).collect::<Vec<Instruction>>();
        assert_eq!(actual, expected_ast)
    }

    #[test]
    fn parse_empty_src() {
        assert_ast_eq("", &[])
    }

    #[test]
    fn parse_empty_line() {
        assert_ast_eq("\n", &[])
    }

    #[test]
    fn parse_nop() {
        parse_nullary_instruction("nop")
    }

    #[test]
    fn parse_nop_after_whitespace() {
        assert_ast_eq("    nop", &[inst("nop", &[])])
    }

    #[test]
    fn parse_halt() {
        parse_nullary_instruction("halt")
    }

    #[test]
    fn parse_stop() {
        parse_nullary_instruction("stop")
    }

    fn parse_nullary_instruction(src: &str) {
        assert_ast_eq(src, &[inst(src, &[])])
    }

    const BC: Operand = Operand::RegisterPair(RegisterPair::Bc);

    #[test]
    fn parse_push_bc() {
        assert_ast_eq("push bc", &[inst("push", &[BC])])
    }

    #[test]
    fn parse_ld_a_a() {
        assert_ast_eq("ld a, a", &[inst("ld", &[A, A])])
    }

    #[test]
    fn parse_ld_a_b() {
        assert_ast_eq("ld a, b", &[inst("ld", &[A, B])])
    }

    #[test]
    fn parse_two_instructions() {
        assert_ast_eq("ld a, b\nld a, b", &[
            inst("ld", &[A, B]),
            inst("ld", &[A, B]),
        ])
    }

    #[test]
    fn parse_two_instructions_separated_by_blank_line() {
        assert_ast_eq("ld a, b\n\nld a, b", &[
            inst("ld", &[A, B]),
            inst("ld", &[A, B]),
        ])
    }
}
