//! Types comprising the assembler's diagnostic output.
//!
//! During assembly the assembler may emit any number of diagnostic messages, each of which is
//! encoded in a [`Diagnostic`](struct.Diagnostic.html) along with all the information necessary to
//! present it to the end user.

pub(crate) use self::message::{KeywordOperandCategory, Message, ValueKind};
pub use crate::codebase::{LineNumber, TextPosition, TextRange};

use super::CompositeSession;

use crate::codebase::{BufId, BufRange, FileCodebase, FileSystem, TextBuf, TextCache};
use crate::session::lex::StringSource;
use crate::span::*;

#[cfg(test)]
use std::cell::RefCell;
use std::ops::Range;

mod message;

#[cfg(test)]
pub(crate) use self::mock::*;

pub(crate) trait DiagnosticsSystem
where
    Self: SpanSystem + Diagnostics<<Self as SpanSource>::Span>,
{
}

pub(crate) trait Diagnostics<S>
where
    Self: MergeSpans<S> + BackendDiagnostics<S>,
{
}

pub(crate) trait BackendDiagnostics<S>
where
    Self: StripSpan<S> + EmitDiag<S, <Self as StripSpan<S>>::Stripped>,
{
}

impl<T, S> BackendDiagnostics<S> for T where
    T: StripSpan<S> + EmitDiag<S, <T as StripSpan<S>>::Stripped>
{
}

impl<T, S> Diagnostics<S> for T where T: MergeSpans<S> + BackendDiagnostics<S> {}

pub trait DiagnosticsOutput {
    fn emit(&mut self, diagnostic: Diagnostic);
}

pub(crate) trait EmitDiag<S, T> {
    fn emit_diag(&mut self, diag: impl Into<CompactDiag<S, T>>);
}

pub(crate) struct OutputForwarder<'a> {
    pub output: &'a mut dyn FnMut(Diagnostic),
}

type Span = RcSpan<BufId, BufRange>;

impl<'a> SpanSource for OutputForwarder<'a> {
    type Span = Span;
}

impl<'a, F, R, I, M, N, B> EmitDiag<Span, StrippedBufSpan<BufId, BufRange>>
    for CompositeSession<FileCodebase<'a, F>, R, I, M, N, B, OutputForwarder<'a>>
where
    F: FileSystem + ?Sized,
    R: SpanSource,
    I: StringSource,
{
    fn emit_diag(&mut self, diag: impl Into<CompactDiag<Span, StrippedBufSpan<BufId, BufRange>>>) {
        (self.diagnostics.output)(diag.into().expand().render(&self.codebase.cache.borrow()))
    }
}

pub(crate) struct DiagnosticsView<'a, C, R, D> {
    pub codebase: &'a mut C,
    pub registry: &'a mut R,
    pub diagnostics: &'a mut D,
}

impl<'a, C, R, D, S> MergeSpans<S> for DiagnosticsView<'a, C, R, D>
where
    R: MergeSpans<S>,
{
    fn merge_spans(&mut self, left: &S, right: &S) -> S {
        self.registry.merge_spans(left, right)
    }
}

impl<'a, C, R, D, S> StripSpan<S> for DiagnosticsView<'a, C, R, D>
where
    R: StripSpan<S>,
{
    type Stripped = R::Stripped;

    fn strip_span(&mut self, span: &S) -> Self::Stripped {
        self.registry.strip_span(span)
    }
}

impl<'a, 'b, F: FileSystem + ?Sized, R> EmitDiag<Span, StrippedBufSpan<BufId, BufRange>>
    for DiagnosticsView<'b, FileCodebase<'a, F>, R, OutputForwarder<'a>>
{
    fn emit_diag(&mut self, diag: impl Into<CompactDiag<Span, StrippedBufSpan<BufId, BufRange>>>) {
        (self.diagnostics.output)(diag.into().expand().render(&self.codebase.cache.borrow()))
    }
}

pub(crate) struct IgnoreDiagnostics;

impl<S: Clone> StripSpan<S> for IgnoreDiagnostics {
    type Stripped = S;

    fn strip_span(&mut self, span: &S) -> Self::Stripped {
        span.clone()
    }
}

impl<S: Clone> EmitDiag<S, S> for IgnoreDiagnostics {
    fn emit_diag(&mut self, _: impl Into<CompactDiag<S>>) {}
}

#[cfg(test)]
pub(crate) struct TestDiagnosticsListener<S> {
    pub diagnostics: RefCell<Vec<CompactDiag<S>>>,
}

#[cfg(test)]
impl<S> TestDiagnosticsListener<S> {
    pub fn new() -> TestDiagnosticsListener<S> {
        TestDiagnosticsListener {
            diagnostics: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl<S: Clone> SpanSource for TestDiagnosticsListener<S> {
    type Span = S;
}

#[cfg(test)]
impl<S: Clone> MergeSpans<S> for TestDiagnosticsListener<S> {
    fn merge_spans(&mut self, _left: &S, _right: &S) -> S {
        unimplemented!()
    }
}

#[cfg(test)]
impl<S: Clone> StripSpan<S> for TestDiagnosticsListener<S> {
    type Stripped = S;

    fn strip_span(&mut self, span: &S) -> Self::Stripped {
        span.clone()
    }
}

#[cfg(test)]
impl<'a, C, R, S> EmitDiag<S, S> for DiagnosticsView<'a, C, R, TestDiagnosticsListener<S>> {
    fn emit_diag(&mut self, diag: impl Into<CompactDiag<S>>) {
        self.diagnostics.diagnostics.borrow_mut().push(diag.into())
    }
}

#[cfg(test)]
impl<C, R: SpanSource, I: StringSource, M, N, B, S> EmitDiag<S, S>
    for CompositeSession<C, R, I, M, N, B, TestDiagnosticsListener<S>>
{
    fn emit_diag(&mut self, diag: impl Into<CompactDiag<S>>) {
        self.diagnostics.diagnostics.borrow_mut().push(diag.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompactDiag<S, R = S> {
    main: CompactClause<S, R>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompactClause<S, R> {
    pub message: Message<R>,
    pub highlight: S,
}

impl<S, R> From<CompactClause<S, R>> for CompactDiag<S, R> {
    fn from(clause: CompactClause<S, R>) -> Self {
        CompactDiag { main: clause }
    }
}

impl<R> Message<R> {
    pub(crate) fn at<S>(self, highlight: S) -> CompactClause<S, R> {
        CompactClause {
            message: self,
            highlight,
        }
    }
}

#[derive(Debug, PartialEq)]
struct ExpandedDiagnostic<S, B, R> {
    clauses: Vec<ExpandedDiagnosticClause<S, B, R>>,
}

#[derive(Debug, PartialEq)]
struct ExpandedDiagnosticClause<S, B, R> {
    buf_id: B,
    tag: Tag,
    message: Message<S>,
    location: Option<R>,
}

impl<B: Clone, T: Clone> CompactDiag<RcSpan<B, Range<T>>, StrippedBufSpan<B, Range<T>>> {
    fn expand(self) -> ExpandedDiagnostic<StrippedBufSpan<B, Range<T>>, B, Range<T>> {
        let StrippedBufSpan { buf_id, range } = self.main.highlight.to_stripped();
        let main_clause = ExpandedDiagnosticClause {
            buf_id,
            tag: Tag::Error,
            message: self.main.message,
            location: Some(range),
        };
        let mut clauses = vec![main_clause];
        if let Some(note) = mk_called_here_clause(&self.main.highlight) {
            clauses.push(note)
        }
        ExpandedDiagnostic { clauses }
    }
}

type BufSnippetClause<B, T> = ExpandedDiagnosticClause<StrippedBufSpan<B, Range<T>>, B, Range<T>>;

fn mk_called_here_clause<B: Clone, T: Clone>(
    span: &RcSpan<B, Range<T>>,
) -> Option<BufSnippetClause<B, T>> {
    let call = if let ModularSpan::Macro(MacroSpan { context, .. }) = span {
        context.name.clone()
    } else {
        return None;
    };
    let stripped = call.to_stripped();
    Some(ExpandedDiagnosticClause {
        buf_id: stripped.buf_id.clone(),
        tag: Tag::Note,
        location: Some(stripped.range.clone()),
        message: Message::CalledHere { name: stripped },
    })
}

/// A full description of an assembler diagnostic.
///
/// A [`Diagnostic`](struct.Diagnostic.html) contains all the information required to display a
/// meaningful diagnostic message to a user. It consists of one or more
/// [`Clause`](struct.Clause.html)s, where the first one describes the object of the diagnostic and
/// the rest provide more context if necessary.
#[derive(Debug, PartialEq)]
pub struct Diagnostic {
    pub clauses: Vec<Clause>,
}

/// A single clause of an assembler diagnostic.
///
/// A [`Clause`](struct.Clause.html) contains a message that's relevant to a particular source file
/// or a part thereof. In addition it may include an excerpt from said file, as well as an optional
/// highlight within it.
#[derive(Debug, PartialEq)]
pub struct Clause {
    pub file: String,
    pub tag: Tag,
    pub message: String,
    pub excerpt: Option<Excerpt>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tag {
    Error,
    Note,
}

/// A code excerpt with an optional highlight.
///
/// An [`Excerpt`](struct.Excerpt.html) contains a single line of code meant to provide context for
/// a diagnostic clause. The optional highlight can pinpoint the interesting part of the line.
#[derive(Debug, PartialEq)]
pub struct Excerpt {
    pub line: LineNumber,
    pub source: String,
    pub highlight: Option<TextRange>,
}

pub(crate) fn mk_diagnostic(
    file: impl Into<String>,
    message: &Message<StrippedBufSpan<BufId, BufRange>>,
) -> Diagnostic {
    Diagnostic {
        clauses: vec![Clause {
            file: file.into(),
            tag: Tag::Error,
            message: message.render(&TextCache::new()),
            excerpt: None,
        }],
    }
}

impl ExpandedDiagnostic<StrippedBufSpan<BufId, BufRange>, BufId, BufRange> {
    fn render(&self, codebase: &TextCache) -> Diagnostic {
        Diagnostic {
            clauses: self
                .clauses
                .iter()
                .map(|clause| clause.render(codebase))
                .collect(),
        }
    }
}

impl ExpandedDiagnosticClause<StrippedBufSpan<BufId, BufRange>, BufId, BufRange> {
    fn render(&self, codebase: &TextCache) -> Clause {
        let buf = codebase.buf(self.buf_id);
        let excerpt = self.location.as_ref().map(|range| {
            let highlight = buf.text_range(&range);
            let source = buf
                .lines(highlight.start.line..=highlight.end.line)
                .next()
                .map(|(_, line)| line.trim_end())
                .unwrap()
                .into();
            Excerpt {
                line: highlight.start.line.into(),
                source,
                highlight: Some(highlight),
            }
        });
        Clause {
            file: buf.name().into(),
            tag: self.tag,
            message: self.message.render(codebase),
            excerpt,
        }
    }
}

#[cfg(test)]
mod mock {
    use super::*;

    use crate::log::Log;

    use std::fmt::Debug;
    use std::marker::PhantomData;

    pub(crate) trait Merge: Sized {
        fn merge(left: impl Into<Self>, right: impl Into<Self>) -> Self;
    }

    #[derive(Clone, Debug, PartialEq)]
    pub(crate) enum MockSpan<T> {
        Basic(T),
        Merge(Box<Self>, Box<Self>),
    }

    impl<T> From<T> for MockSpan<T> {
        fn from(token: T) -> Self {
            MockSpan::Basic(token)
        }
    }

    impl Merge for () {
        fn merge(_: impl Into<Self>, _: impl Into<Self>) -> Self {}
    }

    impl<T> Merge for MockSpan<T> {
        fn merge(left: impl Into<Self>, right: impl Into<Self>) -> Self {
            MockSpan::Merge(Box::new(left.into()), Box::new(right.into()))
        }
    }

    pub(crate) struct MockDiagnostics<T, S> {
        log: Log<T>,
        _span: PhantomData<S>,
    }

    impl<T, S> MockDiagnostics<T, S> {
        pub fn new(log: Log<T>) -> Self {
            Self {
                log,
                _span: PhantomData,
            }
        }
    }

    impl<T, S> Clone for MockDiagnostics<T, S> {
        fn clone(&self) -> Self {
            Self::new(self.log.clone())
        }
    }

    #[derive(Debug, PartialEq)]
    pub(crate) enum DiagnosticsEvent<S> {
        EmitDiag(CompactDiag<S>),
    }

    impl<S> From<CompactDiag<S>> for DiagnosticsEvent<S> {
        fn from(diag: CompactDiag<S>) -> Self {
            DiagnosticsEvent::EmitDiag(diag)
        }
    }

    impl<T, S: Clone> SpanSource for MockDiagnostics<T, S> {
        type Span = S;
    }

    impl<T, S: Clone> StripSpan<S> for MockDiagnostics<T, S> {
        type Stripped = S;

        fn strip_span(&mut self, span: &S) -> Self::Stripped {
            span.clone()
        }
    }

    impl<T, S: Clone + Merge> MergeSpans<S> for MockDiagnostics<T, S> {
        fn merge_spans(&mut self, left: &S, right: &S) -> S {
            S::merge(left.clone(), right.clone())
        }
    }

    impl<C, R, I, M, N, B, T, S> EmitDiag<S, S>
        for CompositeSession<C, R, I, M, N, B, MockDiagnostics<T, S>>
    where
        R: SpanSource,
        I: StringSource,
        T: From<DiagnosticsEvent<S>>,
        S: Clone,
    {
        fn emit_diag(&mut self, diag: impl Into<CompactDiag<S>>) {
            self.diagnostics
                .log
                .push(DiagnosticsEvent::EmitDiag(diag.into()))
        }
    }

    impl<'a, C, R, T, S> EmitDiag<S, S> for DiagnosticsView<'a, C, R, MockDiagnostics<T, S>>
    where
        T: From<DiagnosticsEvent<S>>,
    {
        fn emit_diag(&mut self, diag: impl Into<CompactDiag<S>>) {
            self.diagnostics
                .log
                .push(DiagnosticsEvent::EmitDiag(diag.into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codebase::TextPosition;
    use crate::span::BufContextData;
    use std::rc::Rc;

    static DUMMY_FILE: &str = "/my/file";

    #[test]
    fn mk_message_for_not_a_mnemonic() {
        let mut codebase = TextCache::new();
        let src = "    nop\n    my_macro a, $12\n\n";
        let buf_id = codebase.add_src_buf(DUMMY_FILE, src);
        let range = 12..20;
        let token_ref = ModularSpan::Buf(BufSpan {
            range: range.clone(),
            context: Rc::new(BufContextData {
                buf_id,
                included_from: None,
            }),
        });
        let diagnostic = CompactDiag::from(
            Message::NotAMnemonic {
                name: StrippedBufSpan { buf_id, range },
            }
            .at(token_ref),
        );
        assert_eq!(
            diagnostic.expand().render(&codebase),
            Diagnostic {
                clauses: vec![Clause {
                    file: DUMMY_FILE.to_string(),
                    tag: Tag::Error,
                    message: "`my_macro` is not a mnemonic".to_string(),
                    excerpt: Some(Excerpt {
                        line: LineNumber(2),
                        source: "    my_macro a, $12".to_string(),
                        highlight: mk_highlight(LineNumber(2), 4, 12),
                    })
                }]
            }
        )
    }

    #[test]
    fn expect_1_operand() {
        let message = Message::OperandCount {
            actual: 0,
            expected: 1,
        };
        assert_eq!(
            message.render(&TextCache::new()),
            "expected 1 operand, found 0"
        )
    }

    #[test]
    fn expand_error_in_macro() {
        let buf_context = &Rc::new(BufContextData {
            buf_id: (),
            included_from: None,
        });
        let macro_def = Rc::new(MacroDefSpans {
            name: ModularSpan::Buf(BufSpan {
                range: 0..1,
                context: Rc::clone(buf_context),
            }),
            params: vec![],
            body: vec![ModularSpan::Buf(BufSpan {
                range: 2..3,
                context: Rc::clone(buf_context),
            })],
        });
        let call_range = 10..11;
        let context = RcMacroCall::new(ModularMacroCall {
            name: ModularSpan::Buf(BufSpan {
                range: call_range.clone(),
                context: Rc::clone(buf_context),
            }),
            args: vec![],
            def: macro_def,
        });
        let position = MacroExpansionPos {
            token: 0,
            param_expansion: None,
        };
        let span = ModularSpan::Macro(MacroSpan {
            range: position.clone()..=position,
            context,
        });
        let message = Message::AfOutsideStackOperation;
        let compact = CompactDiag::from(message.clone().at(span));
        let expected = ExpandedDiagnostic {
            clauses: vec![
                ExpandedDiagnosticClause {
                    buf_id: (),
                    tag: Tag::Error,
                    message,
                    location: Some(2..3),
                },
                ExpandedDiagnosticClause {
                    buf_id: (),
                    tag: Tag::Note,
                    message: Message::CalledHere {
                        name: StrippedBufSpan {
                            buf_id: (),
                            range: call_range,
                        },
                    },
                    location: Some(10..11),
                },
            ],
        };
        assert_eq!(compact.expand(), expected)
    }

    fn mk_highlight(line_number: LineNumber, start: usize, end: usize) -> Option<TextRange> {
        Some(TextRange {
            start: TextPosition {
                line: line_number.into(),
                column_index: start,
            },
            end: TextPosition {
                line: line_number.into(),
                column_index: end,
            },
        })
    }
}
