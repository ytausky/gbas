pub use self::lexer::{LexError, Lexer};
pub(crate) use self::parser::ParseTokenStream;

use std::fmt::Debug;

pub mod actions;
mod lexer;
pub mod parser;

#[derive(Clone, Debug, PartialEq)]
pub enum Token<I, L> {
    Ident(I),
    Label(I),
    Literal(L),
    Sigil(Sigil),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sigil {
    Comma,
    Dot,
    Eos,
    Eol,
    EqEq,
    LParen,
    Minus,
    Pipe,
    Plus,
    RParen,
    Slash,
    Star,
}

impl<I, L> From<Sigil> for Token<I, L> {
    fn from(sigil: Sigil) -> Self {
        Token::Sigil(sigil)
    }
}

pub trait IdentSource {
    type Ident: Clone + Debug + PartialEq + AsRef<str>;
}

pub trait IdentFactory: IdentSource {
    fn mk_ident(&mut self, spelling: &str) -> Self::Ident;
}

#[cfg(test)]
impl<I: Clone + Debug + PartialEq + AsRef<str>, F: for<'a> Fn(&'a str) -> I> IdentSource for F {
    type Ident = I;
}

#[cfg(test)]
impl<I: Clone + Debug + PartialEq + AsRef<str>, F: for<'a> Fn(&'a str) -> I> IdentFactory for F {
    fn mk_ident(&mut self, spelling: &str) -> Self::Ident {
        self(spelling)
    }
}
