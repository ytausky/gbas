use codebase::{BufId, BufRange, LineNumber, TextBuf, TextCache, TextRange};
use std::{cell::RefCell, cmp, fmt, rc::Rc};
use Width;

pub trait Span: Clone + fmt::Debug {
    fn extend(&self, other: &Self) -> Self;
}

#[cfg(test)]
impl Span for () {
    fn extend(&self, _: &Self) -> Self {}
}

pub trait Source {
    type Span: Span;
    fn span(&self) -> Self::Span;
}

pub trait TokenTracker {
    type Span: Span;
    type BufContext: Clone + LexemeRefFactory<Span = Self::Span>;
    fn mk_buf_context(
        &mut self,
        buf_id: BufId,
        included_from: Option<Self::Span>,
    ) -> Self::BufContext;
}

pub trait LexemeRefFactory {
    type Span;
    fn mk_lexeme_ref(&self, range: BufRange) -> Self::Span;
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenRefData {
    Lexeme {
        range: BufRange,
        context: Rc<BufContextData>,
    },
}

#[derive(Debug, PartialEq)]
pub struct BufContextData {
    buf_id: BufId,
    included_from: Option<TokenRefData>,
}

pub struct SimpleTokenTracker;

impl TokenTracker for SimpleTokenTracker {
    type Span = TokenRefData;
    type BufContext = SimpleBufTokenRefFactory;
    fn mk_buf_context(
        &mut self,
        buf_id: BufId,
        included_from: Option<Self::Span>,
    ) -> Self::BufContext {
        let context = Rc::new(BufContextData {
            buf_id,
            included_from,
        });
        SimpleBufTokenRefFactory { context }
    }
}

#[derive(Clone)]
pub struct SimpleBufTokenRefFactory {
    context: Rc<BufContextData>,
}

impl LexemeRefFactory for SimpleBufTokenRefFactory {
    type Span = TokenRefData;
    fn mk_lexeme_ref(&self, range: BufRange) -> Self::Span {
        TokenRefData::Lexeme {
            range,
            context: self.context.clone(),
        }
    }
}

impl Span for TokenRefData {
    fn extend(&self, other: &Self) -> Self {
        use diagnostics::TokenRefData::*;
        match (self, other) {
            (
                Lexeme { range, context },
                Lexeme {
                    range: other_range,
                    context: other_context,
                },
            )
                if Rc::ptr_eq(context, other_context) =>
            {
                Lexeme {
                    range: cmp::min(range.start, other_range.start)
                        ..cmp::max(range.end, other_range.end),
                    context: (*context).clone(),
                }
            }
            _ => panic!(),
        }
    }
}

pub trait DiagnosticsListener<TR> {
    fn emit_diagnostic(&self, diagnostic: Diagnostic<TR>);
}

#[cfg(test)]
pub struct IgnoreDiagnostics;

#[cfg(test)]
impl<SR> DiagnosticsListener<SR> for IgnoreDiagnostics {
    fn emit_diagnostic(&self, _: Diagnostic<SR>) {}
}

#[cfg(test)]
pub struct TestDiagnosticsListener {
    pub diagnostics: RefCell<Vec<Diagnostic<()>>>,
}

#[cfg(test)]
impl TestDiagnosticsListener {
    pub fn new() -> TestDiagnosticsListener {
        TestDiagnosticsListener {
            diagnostics: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl DiagnosticsListener<()> for TestDiagnosticsListener {
    fn emit_diagnostic(&self, diagnostic: Diagnostic<()>) {
        self.diagnostics.borrow_mut().push(diagnostic)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic<S> {
    pub message: Message,
    pub spans: Vec<S>,
    pub highlight: S,
}

impl<S> Diagnostic<S> {
    pub fn new(
        message: Message,
        spans: impl IntoIterator<Item = S>,
        highlight: S,
    ) -> Diagnostic<S> {
        Diagnostic {
            message,
            spans: spans.into_iter().collect(),
            highlight,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    AlwaysUnconditional,
    CannotDereference { category: KeywordOperandCategory },
    DestMustBeA,
    DestMustBeHl,
    IncompatibleOperand,
    KeywordInExpr,
    MissingTarget,
    OperandCount { actual: usize, expected: usize },
    StringInInstruction,
    UndefinedMacro { name: String },
    UnexpectedEof,
    UnexpectedToken,
    UnresolvedSymbol { symbol: String },
    ValueOutOfRange { value: i32, width: Width },
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeywordOperandCategory {
    Reg,
    RegPair,
    ConditionCode,
}

impl Message {
    fn render<'a>(&self, snippets: impl IntoIterator<Item = &'a str>) -> String {
        use diagnostics::Message::*;
        let mut snippets = snippets.into_iter();
        let string = match self {
            AlwaysUnconditional => "instruction cannot be made conditional".into(),
            CannotDereference { category } => format!(
                "{} `{}` cannot be dereferenced",
                category,
                snippets.next().unwrap(),
            ),
            DestMustBeA => "destination of ALU operation must be `a`".into(),
            DestMustBeHl => "destination operand must be `hl`".into(),
            IncompatibleOperand => "operand cannot be used with this instruction".into(),
            KeywordInExpr => format!(
                "keyword `{}` cannot appear in expression",
                snippets.next().unwrap(),
            ),
            MissingTarget => "branch instruction requires target".into(),
            OperandCount { actual, expected } => format!(
                "expected {} operand{}, found {}",
                expected,
                pluralize(*expected),
                actual
            ),
            StringInInstruction => "strings cannot appear in instruction operands".into(),
            UndefinedMacro { name } => format!("invocation of undefined macro `{}`", name),
            UnexpectedEof => "unexpected end of file".into(),
            UnexpectedToken => format!(
                "encountered unexpected token `{}`",
                snippets.next().unwrap(),
            ),
            UnresolvedSymbol { symbol } => format!("symbol `{}` could not be resolved", symbol),
            ValueOutOfRange { value, width } => {
                format!("value {} cannot be represented in a {}", value, width)
            }
        };
        assert_eq!(snippets.next(), None);
        string
    }
}

impl fmt::Display for KeywordOperandCategory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeywordOperandCategory::Reg => f.write_str("register"),
            KeywordOperandCategory::RegPair => f.write_str("register pair"),
            KeywordOperandCategory::ConditionCode => f.write_str("condition code"),
        }
    }
}

fn pluralize(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

impl fmt::Display for Width {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Width::Byte => f.write_str("byte"),
            Width::Word => f.write_str("word"),
        }
    }
}

pub struct TerminalDiagnostics<'a> {
    codebase: &'a RefCell<TextCache>,
}

impl<'a> TerminalDiagnostics<'a> {
    pub fn new(codebase: &'a RefCell<TextCache>) -> TerminalDiagnostics<'a> {
        TerminalDiagnostics { codebase }
    }
}

impl<'a> DiagnosticsListener<TokenRefData> for TerminalDiagnostics<'a> {
    fn emit_diagnostic(&self, diagnostic: Diagnostic<TokenRefData>) {
        let codebase = self.codebase.borrow();
        let elaborated_diagnostic = elaborate(&diagnostic, &codebase);
        print!("{}", elaborated_diagnostic)
    }
}

#[derive(Debug, PartialEq)]
struct ElaboratedDiagnostic<'a> {
    text: String,
    buf_name: &'a str,
    highlight: TextRange,
    src_line: &'a str,
}

fn elaborate<'a>(
    diagnostic: &Diagnostic<TokenRefData>,
    codebase: &'a TextCache,
) -> ElaboratedDiagnostic<'a> {
    match diagnostic.highlight {
        TokenRefData::Lexeme {
            ref range,
            ref context,
        } => {
            let buf = codebase.buf(context.buf_id);
            let text_range = buf.text_range(&range);
            let (_, src_line) = buf
                .lines(text_range.start.line..=text_range.end.line)
                .next()
                .unwrap();
            let snippets = diagnostic
                .spans
                .iter()
                .map(|span| mk_snippet(codebase, span));
            ElaboratedDiagnostic {
                text: diagnostic.message.render(snippets),
                buf_name: buf.name(),
                highlight: text_range,
                src_line,
            }
        }
    }
}

impl<'a> fmt::Display for ElaboratedDiagnostic<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        assert_eq!(self.highlight.start.line, self.highlight.end.line);
        let line_number: LineNumber = self.highlight.start.line.into();
        let mut highlight = String::new();
        let space_count = self.highlight.start.column_index;
        let tilde_count = match self.highlight.end.column_index - space_count {
            0 => 1,
            n => n,
        };
        for _ in 0..space_count {
            highlight.push(' ');
        }
        for _ in 0..tilde_count {
            highlight.push('~');
        }
        writeln!(
            f,
            "{}:{}: error: {}\n{}\n{}",
            self.buf_name, line_number, self.text, self.src_line, highlight
        )
    }
}

fn mk_snippet<'a>(codebase: &'a TextCache, span: &TokenRefData) -> &'a str {
    match span {
        TokenRefData::Lexeme { range, context } => {
            &codebase.buf(context.buf_id).as_str()[range.start..range.end]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase::TextPosition;

    static DUMMY_FILE: &str = "/my/file";

    #[test]
    fn extend_span() {
        let mut codebase = TextCache::new();
        let src = "left right";
        let buf_id = codebase.add_src_buf(DUMMY_FILE, src);
        let context = Rc::new(BufContextData {
            buf_id,
            included_from: None,
        });
        let left = TokenRefData::Lexeme {
            range: BufRange::from(0..4),
            context: context.clone(),
        };
        let right = TokenRefData::Lexeme {
            range: BufRange::from(5..10),
            context: context.clone(),
        };
        let combined = left.extend(&right);
        assert_eq!(
            combined,
            TokenRefData::Lexeme {
                range: BufRange::from(0..10),
                context
            }
        )
    }

    #[test]
    fn get_snippet() {
        let mut codebase = TextCache::new();
        let src = "add snippet, my";
        let buf_id = codebase.add_src_buf(DUMMY_FILE, src);
        let context = Rc::new(BufContextData {
            buf_id,
            included_from: None,
        });
        let span = TokenRefData::Lexeme {
            range: BufRange::from(4..11),
            context: context.clone(),
        };
        assert_eq!(mk_snippet(&codebase, &span), "snippet")
    }

    #[test]
    fn mk_message_for_undefined_macro() {
        let mut codebase = TextCache::new();
        let src = "    nop\n    my_macro a, $12\n\n";
        let buf_id = codebase.add_src_buf(DUMMY_FILE, src);
        let range = BufRange::from(12..20);
        let token_ref = TokenRefData::Lexeme {
            range,
            context: Rc::new(BufContextData {
                buf_id,
                included_from: None,
            }),
        };
        let diagnostic = Diagnostic {
            message: Message::UndefinedMacro {
                name: "my_macro".to_string(),
            },
            spans: Vec::new(),
            highlight: token_ref,
        };
        let elaborated_diagnostic = elaborate(&diagnostic, &codebase);
        assert_eq!(
            elaborated_diagnostic,
            ElaboratedDiagnostic {
                text: "invocation of undefined macro `my_macro`".to_string(),
                buf_name: DUMMY_FILE,
                highlight: mk_highlight(LineNumber(2), 4, 12),
                src_line: "    my_macro a, $12",
            }
        )
    }

    #[test]
    fn render_elaborated_diagnostic() {
        let elaborated_diagnostic = ElaboratedDiagnostic {
            text: "invocation of undefined macro `my_macro`".to_string(),
            buf_name: DUMMY_FILE,
            highlight: mk_highlight(LineNumber(2), 4, 12),
            src_line: "    my_macro a, $12",
        };
        let expected = r"/my/file:2: error: invocation of undefined macro `my_macro`
    my_macro a, $12
    ~~~~~~~~
";
        assert_eq!(elaborated_diagnostic.to_string(), expected)
    }

    #[test]
    fn highlight_eof_with_one_tilde() {
        let elaborated = ElaboratedDiagnostic {
            text: "unexpected end of file".into(),
            buf_name: DUMMY_FILE,
            highlight: mk_highlight(LineNumber(2), 5, 5),
            src_line: "dummy",
        };
        let expected = r"/my/file:2: error: unexpected end of file
dummy
     ~
";
        assert_eq!(elaborated.to_string(), expected)
    }

    #[test]
    fn expect_1_operand() {
        let message = Message::OperandCount {
            actual: 0,
            expected: 1,
        };
        assert_eq!(message.render(Vec::new()), "expected 1 operand, found 0")
    }

    fn mk_highlight(line_number: LineNumber, start: usize, end: usize) -> TextRange {
        TextRange {
            start: TextPosition {
                line: line_number.into(),
                column_index: start,
            },
            end: TextPosition {
                line: line_number.into(),
                column_index: end,
            },
        }
    }
}
