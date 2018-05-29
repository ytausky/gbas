#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Add,
    And,
    Call,
    Cp,
    Db,
    Dec,
    Di,
    Dw,
    Ei,
    Halt,
    Inc,
    Include,
    Jp,
    Jr,
    Ld,
    Ldh,
    Nop,
    Pop,
    Push,
    Ret,
    Reti,
    Stop,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operand {
    A,
    Af,
    B,
    Bc,
    C,
    D,
    De,
    E,
    H,
    Hl,
    L,
    Nc,
    Nz,
    Sp,
    Z,
}
