#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Add,
    And,
    Call,
    Cp,
    Daa,
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
    Nop,
    Org,
    Pop,
    Push,
    Ret,
    Reti,
    Stop,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OperandKeyword {
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
