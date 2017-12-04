#[cfg(test)]
use ast;

#[cfg(test)]
fn parse_src(src: &str) -> ast::AssemblyCommands {
    let trimmed_src = src.trim();
    if let Some(first_space) = trimmed_src.find(' ') {
        let (mnemonic, operands) = trimmed_src.split_at(first_space);
        vec![make_emit_bytes(mnemonic, &parse_operands(operands))]
    } else {
        match trimmed_src {
            "nop" | "halt" | "stop" => vec![make_emit_bytes(trimmed_src, &[])],
            _ => vec![]
        }
    }
}

#[cfg(test)]
fn parse_operands(src: &str) -> Vec<ast::Operand> {
    let mut operands = vec![];
    for op in src.split(',') {
        match op.trim() {
            "a" => operands.push(ast::Operand::Register(ast::Register::A)),
            "bc" => operands.push(ast::Operand::RegisterPair(ast::RegisterPair::Bc)),
            _ => panic!(),
        }
    }
    operands
}

#[cfg(test)]
fn make_emit_bytes(mnemonic: &str, operands: &[ast::Operand]) -> ast::EmitBytes {
    ast::EmitBytes {
        mnemonic: mnemonic.to_owned(),
        operands: operands.iter().map(|&x| x).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_ast_eq(src: &str, commands: &[(&str, &[ast::Operand])]) {
        let expected_ast = commands.iter()
                                   .map(|&(mnemonic, operands)| make_emit_bytes(mnemonic, operands))
                                   .collect::<Vec<ast::EmitBytes>>();
        assert_eq!(parse_src(src), expected_ast)
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
        assert_ast_eq("    nop", &[("nop", &[])])
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
        assert_ast_eq(src, &[(src, &[])])
    }

    const BC: ast::Operand = ast::Operand::RegisterPair(ast::RegisterPair::Bc);

    #[test]
    fn parse_push_bc() {
        assert_ast_eq("push bc", &[("push", &[BC])])
    }

    const A: ast::Operand = ast::Operand::Register(ast::Register::A);

    #[test]
    fn parse_ld_a_a() {
        assert_ast_eq("ld a, a", &[("ld", &[A, A])])
    }
}
