pub use crate::syntax::Token;

use self::backend::*;
use self::macros::{MacroDefData, MacroTableEntry};
use self::session::*;

use crate::codebase::{BufId, Codebase, CodebaseError};
use crate::diag::*;
use crate::name::{BiLevelNameTable, Ident};
use crate::span::BufContext;
use crate::syntax::lexer::{LexError, Lexer};
use crate::syntax::*;

use std::rc::Rc;

#[cfg(test)]
pub use self::mock::*;

pub mod backend;
mod macros;
mod semantics;
mod session;

pub(crate) trait Assemble<D, S>
where
    D: Diagnostics,
    Self: Backend<
        Ident<String>,
        D::Span,
        BiLevelNameTable<MacroTableEntry<D::MacroDefId, Rc<MacroDefData<String>>>, S>,
    >,
{
    fn assemble<C: Codebase>(
        &mut self,
        name: &str,
        codebase: &C,
        diagnostics: &mut D,
    ) -> Result<(), CodebaseError> {
        let mut file_parser = CodebaseAnalyzer::new(codebase);
        let mut analyzer = semantics::SemanticAnalyzer;
        let mut names = BiLevelNameTable::new();
        let mut session = CompositeSession::new(
            &mut file_parser,
            &mut analyzer,
            self,
            &mut names,
            diagnostics,
        );
        session.analyze_file(name.into())
    }
}

impl<B, D, S> Assemble<D, S> for B
where
    D: Diagnostics,
    B: Backend<
        Ident<String>,
        D::Span,
        BiLevelNameTable<MacroTableEntry<D::MacroDefId, Rc<MacroDefData<String>>>, S>,
    >,
{
}

type LexItem<T, S> = (Result<SemanticToken<T>, LexError>, S);

pub(crate) type SemanticToken<T> = Token<Ident<T>, Literal<T>, Command>;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Literal<S> {
    Operand(Operand),
    Number(i32),
    String(S),
}

pub(crate) trait Lex<D: Diagnostics> {
    type StringRef: Clone + Eq;
    type TokenIter: Iterator<Item = LexItem<Self::StringRef, D::Span>>;

    fn lex_file(
        &mut self,
        path: Self::StringRef,
        diagnostics: &mut D,
    ) -> Result<Self::TokenIter, CodebaseError>;
}

struct CodebaseAnalyzer<'a, T: 'a> {
    codebase: &'a T,
}

impl<'a, T: 'a> CodebaseAnalyzer<'a, T>
where
    T: StringRef,
{
    fn new(codebase: &T) -> CodebaseAnalyzer<T> {
        CodebaseAnalyzer { codebase }
    }
}

type TokenSeq<I, S> = Vec<(SemanticToken<I>, S)>;

impl<'a, T, D> Lex<D> for CodebaseAnalyzer<'a, T>
where
    T: Tokenize<D::BufContext> + 'a,
    T::StringRef: AsRef<str>,
    D: Diagnostics,
{
    type StringRef = T::StringRef;
    type TokenIter = T::Tokenized;

    fn lex_file(
        &mut self,
        path: Self::StringRef,
        diagnostics: &mut D,
    ) -> Result<Self::TokenIter, CodebaseError> {
        self.codebase.tokenize_file(path.as_ref(), |buf_id| {
            diagnostics.mk_buf_context(buf_id, None)
        })
    }
}

pub(crate) trait StringRef {
    type StringRef: Clone + Eq;
}

trait Tokenize<C: BufContext>
where
    Self: StringRef,
{
    type Tokenized: Iterator<Item = LexItem<Self::StringRef, C::Span>>;
    fn tokenize_file<F: FnOnce(BufId) -> C>(
        &self,
        filename: &str,
        mk_context: F,
    ) -> Result<Self::Tokenized, CodebaseError>;
}

impl<C: Codebase> StringRef for C {
    type StringRef = String;
}

impl<C: Codebase, B: BufContext> Tokenize<B> for C {
    type Tokenized = TokenizedSrc<B>;

    fn tokenize_file<F: FnOnce(BufId) -> B>(
        &self,
        filename: &str,
        mk_context: F,
    ) -> Result<Self::Tokenized, CodebaseError> {
        let buf_id = self.open(filename)?;
        let rc_src = self.buf(buf_id);
        Ok(TokenizedSrc::new(rc_src, mk_context(buf_id)))
    }
}

struct TokenizedSrc<C> {
    tokens: Lexer<Rc<str>, MkIdent>,
    context: C,
}

type MkIdent = for<'a> fn(&'a str) -> Ident<String>;

impl<C: BufContext> TokenizedSrc<C> {
    fn new(src: Rc<str>, context: C) -> TokenizedSrc<C> {
        TokenizedSrc {
            tokens: crate::syntax::tokenize(src, crate::name::mk_ident),
            context,
        }
    }
}

impl<'a, C: BufContext> Iterator for TokenizedSrc<C> {
    type Item = LexItem<String, C::Span>;

    fn next(&mut self) -> Option<Self::Item> {
        self.tokens
            .next()
            .map(|(t, r)| (t, self.context.mk_span(r)))
    }
}

#[cfg(test)]
mod mock {
    use super::*;

    use std::collections::HashMap;
    use std::vec::IntoIter;

    pub struct MockCodebase<S> {
        files: HashMap<String, Vec<LexItem<String, S>>>,
    }

    impl<S> MockCodebase<S> {
        pub fn new() -> Self {
            MockCodebase {
                files: HashMap::new(),
            }
        }

        pub(crate) fn set_file<I>(&mut self, path: &str, tokens: I)
        where
            I: IntoIterator<Item = LexItem<String, S>>,
        {
            self.files.insert(path.into(), tokens.into_iter().collect());
        }
    }

    impl<'a, D> Lex<D> for MockCodebase<D::Span>
    where
        D: Diagnostics,
    {
        type StringRef = String;
        type TokenIter = IntoIter<LexItem<Self::StringRef, D::Span>>;

        fn lex_file(
            &mut self,
            path: Self::StringRef,
            _diagnostics: &mut D,
        ) -> Result<Self::TokenIter, CodebaseError> {
            Ok(self.files.get(&path).unwrap().clone().into_iter())
        }
    }
}