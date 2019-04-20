use super::{Immediate, NameDef, NameId, Node, Program, Section};

use crate::analysis::backend::*;
use crate::model::Item;

pub struct ProgramBuilder<'a, S> {
    program: &'a mut Program<S>,
    state: Option<BuilderState<S>>,
}

enum BuilderState<S> {
    AnonSectionPrelude { addr: Option<Immediate<S>> },
    Section(usize),
    SectionPrelude(usize),
}

impl<'a, S> ProgramBuilder<'a, S> {
    pub fn new(program: &'a mut Program<S>) -> Self {
        Self {
            program,
            state: Some(BuilderState::AnonSectionPrelude { addr: None }),
        }
    }

    fn push(&mut self, node: Node<S>) {
        self.current_section().items.push(node)
    }

    fn current_section(&mut self) -> &mut Section<S> {
        match self.state.take().unwrap() {
            BuilderState::AnonSectionPrelude { addr } => {
                self.program.add_section(None);
                let index = self.program.sections.len() - 1;
                self.state = Some(BuilderState::Section(index));
                let section = &mut self.program.sections[index];
                section.constraints.addr = addr;
                section
            }
            BuilderState::SectionPrelude(index) | BuilderState::Section(index) => {
                self.state = Some(BuilderState::Section(index));
                &mut self.program.sections[index]
            }
        }
    }
}

impl<'a, S: Clone> PartialBackend<S> for ProgramBuilder<'a, S> {
    type Value = Immediate<S>;

    fn emit_item(&mut self, item: Item<Self::Value>) {
        use super::lowering::Lower;
        item.lower().for_each(|data_item| self.push(data_item))
    }

    fn reserve(&mut self, bytes: Self::Value) {
        self.current_section().items.push(Node::Reserved(bytes))
    }

    fn set_origin(&mut self, addr: Self::Value) {
        match self.state.take().unwrap() {
            BuilderState::SectionPrelude(index) => {
                self.program.sections[index].constraints.addr = Some(addr);
                self.state = Some(BuilderState::SectionPrelude(index))
            }
            _ => self.state = Some(BuilderState::AnonSectionPrelude { addr: Some(addr) }),
        }
    }
}

impl<'a, S: Clone> Backend<S> for ProgramBuilder<'a, S> {
    fn define_fn(&mut self, (name, span): (Self::Name, S), value: Self::Value) {
        let reloc = self.program.alloc_reloc();
        self.program.names.define(name, NameDef::Reloc(reloc));
        self.push(Node::Symbol((name, span), value))
    }
}

impl<'a, S: Clone> AllocName<S> for ProgramBuilder<'a, S> {
    type Name = NameId;

    fn alloc_name(&mut self, _span: S) -> Self::Name {
        self.program.names.alloc()
    }
}

impl<'a, S: Clone> StartSection<NameId, S> for ProgramBuilder<'a, S> {
    fn start_section(&mut self, name: (NameId, S)) {
        let index = self.program.sections.len();
        self.state = Some(BuilderState::SectionPrelude(index));
        self.program.add_section(Some(name.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::diag::{CompactDiagnostic, Message, TestDiagnosticsListener};
    use crate::model::{Atom, BinOp, Instruction, Nullary, Width};
    use crate::program::{BinaryObject, SectionId};
    use std::borrow::Borrow;

    #[test]
    fn new_object_has_no_sections() {
        let object = build_object::<_, ()>(|_| ());
        assert_eq!(object.sections.len(), 0)
    }

    #[test]
    fn no_origin_by_default() {
        let object = build_object::<_, ()>(|builder| builder.push(Node::Byte(0xcd)));
        assert_eq!(object.sections[0].constraints.addr, None)
    }

    #[test]
    fn constrain_origin_determines_origin_of_new_section() {
        let origin: Immediate<_> = 0x3000.into();
        let object = build_object(|builder| {
            builder.set_origin(origin.clone());
            builder.push(Node::Byte(0xcd))
        });
        assert_eq!(object.sections[0].constraints.addr, Some(origin))
    }

    #[test]
    fn start_section_adds_named_section() {
        let mut wrapped_name = None;
        let object = build_object(|builder| {
            let name = builder.alloc_name(());
            builder.start_section((name, ()));
            wrapped_name = Some(name);
        });
        assert_eq!(
            object.names.get(wrapped_name.unwrap()),
            Some(&NameDef::Section(SectionId(0)))
        )
    }

    #[test]
    fn set_origin_in_section_prelude_sets_origin() {
        let origin: Immediate<_> = 0x0150.into();
        let object = build_object(|builder| {
            let name = builder.alloc_name(());
            builder.start_section((name, ()));
            builder.set_origin(origin.clone())
        });
        assert_eq!(object.sections[0].constraints.addr, Some(origin))
    }

    #[test]
    fn push_node_into_named_section() {
        let node = Node::Byte(0x42);
        let object = build_object(|builder| {
            let name = builder.alloc_name(());
            builder.start_section((name, ()));
            builder.push(node.clone())
        });
        assert_eq!(object.sections[0].items, [node])
    }

    fn build_object<F: FnOnce(&mut ProgramBuilder<S>), S>(f: F) -> Program<S> {
        let mut program = Program::new();
        let mut builder = ProgramBuilder::new(&mut program);
        f(&mut builder);
        program
    }

    #[test]
    fn emit_stop() {
        emit_items_and_compare(
            [Item::Instruction(Instruction::Nullary(Nullary::Stop))],
            [0x10, 0x00],
        )
    }

    fn emit_items_and_compare<I, B>(items: I, bytes: B)
    where
        I: Borrow<[Item<Immediate<()>>]>,
        B: Borrow<[u8]>,
    {
        let (object, _) = with_object_builder(|builder| {
            for item in items.borrow() {
                builder.emit_item(item.clone())
            }
        });
        assert_eq!(object.sections.last().unwrap().data, bytes.borrow())
    }

    #[test]
    fn emit_literal_byte_item() {
        emit_items_and_compare([byte_literal(0xff)], [0xff])
    }

    #[test]
    fn emit_two_literal_byte_item() {
        emit_items_and_compare([byte_literal(0x12), byte_literal(0x34)], [0x12, 0x34])
    }

    fn byte_literal(value: i32) -> Item<Immediate<()>> {
        Item::Data(value.into(), Width::Byte)
    }

    #[test]
    fn emit_diagnostic_when_byte_item_out_of_range() {
        test_diagnostic_for_out_of_range_byte(i32::from(i8::min_value()) - 1);
        test_diagnostic_for_out_of_range_byte(i32::from(u8::max_value()) + 1)
    }

    fn test_diagnostic_for_out_of_range_byte(value: i32) {
        let (_, diagnostics) =
            with_object_builder(|builder| builder.emit_item(byte_literal(value)));
        assert_eq!(
            *diagnostics,
            [Message::ValueOutOfRange {
                value,
                width: Width::Byte,
            }
            .at(())
            .into()]
        );
    }

    #[test]
    fn diagnose_unresolved_symbol() {
        let name = "ident";
        let (_, diagnostics) = with_object_builder(|builder| {
            let symbol_id = builder.alloc_name(name.into());
            let mut value: Immediate<_> = Default::default();
            value.push_op(symbol_id, name.into());
            builder.emit_item(word_item(value))
        });
        assert_eq!(*diagnostics, [unresolved(name)]);
    }

    #[test]
    fn diagnose_two_unresolved_symbols_in_one_expr() {
        let name1 = "ident1";
        let name2 = "ident2";
        let (_, diagnostics) = with_object_builder(|builder| {
            let value = {
                let id1 = builder.alloc_name(name1.into());
                let mut value: Immediate<_> = Default::default();
                value.push_op(id1, name1.into());
                let id2 = builder.alloc_name(name2.into());
                value.push_op(id2, name2.into());
                value.push_op(BinOp::Minus, "diff".into());
                value
            };
            builder.emit_item(word_item(value))
        });
        assert_eq!(*diagnostics, [unresolved(name1), unresolved(name2)]);
    }

    #[test]
    fn emit_defined_symbol() {
        let (object, diagnostics) = with_object_builder(|builder| {
            let symbol_id = builder.alloc_name(());
            builder.define_fn((symbol_id, ()), Atom::LocationCounter.into());
            let mut value: Immediate<_> = Default::default();
            value.push_op(symbol_id, ());
            builder.emit_item(word_item(value));
        });
        assert_eq!(*diagnostics, []);
        assert_eq!(object.sections.last().unwrap().data, [0x00, 0x00])
    }

    #[test]
    fn emit_symbol_defined_after_use() {
        let (object, diagnostics) = with_object_builder(|builder| {
            let symbol_id = builder.alloc_name(());
            let mut value: Immediate<_> = Default::default();
            value.push_op(symbol_id, ());
            builder.emit_item(word_item(value));
            builder.define_fn((symbol_id, ()), Atom::LocationCounter.into());
        });
        assert_eq!(*diagnostics, []);
        assert_eq!(object.sections.last().unwrap().data, [0x02, 0x00])
    }

    #[test]
    fn reserve_bytes_in_section() {
        let bytes = 3;
        let program = build_object(|builder| builder.reserve(bytes.into()));
        assert_eq!(program.sections[0].items, [Node::Reserved(bytes.into())])
    }

    fn with_object_builder<S, F>(f: F) -> (BinaryObject, Box<[CompactDiagnostic<S, S>]>)
    where
        S: Clone + 'static,
        F: FnOnce(&mut ProgramBuilder<S>),
    {
        let mut diagnostics = TestDiagnosticsListener::new();
        let object = build_object(f).link(&mut diagnostics);
        let diagnostics = diagnostics.diagnostics.into_inner().into_boxed_slice();
        (object, diagnostics)
    }

    fn word_item<S: Clone>(value: Immediate<S>) -> Item<Immediate<S>> {
        Item::Data(value, Width::Word)
    }

    fn unresolved(symbol: impl Into<String>) -> CompactDiagnostic<String, String> {
        let symbol = symbol.into();
        Message::UnresolvedSymbol {
            symbol: symbol.clone(),
        }
        .at(symbol)
        .into()
    }
}
