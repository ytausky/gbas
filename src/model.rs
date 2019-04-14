use crate::span::Source;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinaryOperator {
    BitwiseOr,
    Division,
    Minus,
    Multiplication,
    Plus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    Byte,
    Word,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item<V: Source> {
    Data(V, Width),
    Instruction(Instruction<V>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Atom<N> {
    Name(N),
    Literal(i32),
    LocationCounter,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction<V: Source> {
    AddHl(Reg16),
    Alu(AluOperation, AluSource<V>),
    Bit(BitOperation, V, SimpleOperand),
    IncDec8(IncDec, SimpleOperand),
    IncDec16(IncDec, Reg16),
    JpDerefHl,
    Branch(Branch<V>, Option<Condition>),
    Ld(Ld<V>),
    Ldhl(V),
    Misc(MiscOperation, SimpleOperand),
    Nullary(Nullary),
    Pop(RegPair),
    Push(RegPair),
    Rst(V),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nullary {
    Cpl,
    Daa,
    Di,
    Ei,
    Halt,
    Nop,
    Reti,
    Rla,
    Rlca,
    Rra,
    Rrca,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AluOperation {
    Add,
    Adc,
    Sub,
    Sbc,
    And,
    Xor,
    Or,
    Cp,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AluSource<V> {
    Simple(SimpleOperand),
    Immediate(V),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BitOperation {
    Bit,
    Set,
    Res,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MiscOperation {
    Rlc,
    Rrc,
    Rl,
    Rr,
    Sla,
    Sra,
    Swap,
    Srl,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimpleOperand {
    A,
    B,
    C,
    D,
    E,
    H,
    L,
    DerefHl,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ld<V> {
    Simple(SimpleOperand, SimpleOperand),
    Special(SpecialLd<V>, Direction),
    SpHl,
    Immediate8(SimpleOperand, V),
    Immediate16(Reg16, V),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpecialLd<V> {
    DerefPtrReg(PtrReg),
    InlineAddr(V),
    RegIndex,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    FromA,
    IntoA,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reg16 {
    Bc,
    De,
    Hl,
    Sp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegPair {
    Bc,
    De,
    Hl,
    Af,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PtrReg {
    Bc,
    De,
    Hli,
    Hld,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Branch<V> {
    Call(V),
    Jp(V),
    Jr(V),
    Ret,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Condition {
    C,
    Nc,
    Nz,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IncDec {
    Inc,
    Dec,
}
