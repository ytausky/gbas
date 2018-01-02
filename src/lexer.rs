use token::Token;

use std::str;

pub struct Lexer<'a> {
    lines: str::Lines<'a>,
    words: Option<str::SplitWhitespace<'a>>,
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        match self.next_in_line() {
            Some(token) => {
                if token == Token::Eol {
                    self.words = None
                };
                Some(token)
            },
            None => {
                self.words = Some(self.lines.next()?.split_whitespace());
                self.next()
            }
        }
    }
}

impl<'a> Lexer<'a> {
    pub fn new(src: &str) -> Lexer {
        Lexer {
            lines: src.lines(),
            words: None,
        }
    }

    fn next_in_line(&mut self) -> Option<Token<'a>> {
        self.words.as_mut().map(|words| words.next().map_or(Token::Eol, |word| Token::Word(word)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use token::Token::*;

    fn assert_eq_tokens(src: &str, expected_tokens: &[Token]) {
        assert_eq!(Lexer::new(src).collect::<Vec<Token>>(), expected_tokens)
    }

    #[test]
    fn lex_empty_str() {
        assert_eq_tokens("", &[])
    }

    #[test]
    fn lex_eol() {
        assert_eq_tokens("\n", &[Eol])
    }
}
