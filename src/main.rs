#[macro_use]
extern crate log;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;
use std::{collections::BTreeMap, num::NonZeroI32, num::NonZeroU64};
use TextNode::Tombstone;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NodeId {
    operation_id: u64,
    client_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ParagraphId {
    operation_id: u64,
    client_id: u64,
}

impl ParagraphId {
    fn from_node_id(node_id: &NodeId) -> Self {
        Self {
            operation_id: node_id.operation_id,
            client_id: node_id.client_id,
        }
    }
}

#[derive(Debug)]
struct UnformattedText {}

#[derive(Debug)]
struct FormattedText {}

// May change one format attribute (e.g. bold), but is still affected by surrounding text on the rest
#[derive(Clone, Debug)]
struct PartiallyFormattedText {
    node_id: NodeId,
    text: String,
    format: TextFormatChange,
}

#[derive(Clone, Debug)]
enum TextFormat {
    Bold = 0,
    Italic = 1,
}

#[derive(Clone, Debug, Default)]
struct TextFormatChange {
    values_to_set: u32,
    value: u32,
}

#[derive(Clone, Debug)]
struct Format {}

#[derive(Clone, Debug)]
struct ParagraphStyle {}

#[derive(Clone, Debug)]
struct NewParagraph {
    node_id: ParagraphId,
    text: Vec<PartiallyFormattedText>,
}

#[derive(Clone, Debug)]
struct TextAnchor {
    at_node: NodeId,

    // Should we even allow 0?
    at_index: Option<u32>, // if None, insert after at_node
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ParagraphAnchorRelativity {
    AtBeginning,
    AtEnd,
}

#[derive(Clone, Debug)]
struct ParagraphAnchor {
    paragraph_id: ParagraphId,
    paragraph_anchor_relativity: ParagraphAnchorRelativity,
}

#[derive(Clone, Debug)]
enum TextOrParagraphAnchor {
    TextAnchor(TextAnchor),
    ParagraphAnchor(ParagraphAnchor),
}

impl TextOrParagraphAnchor {
    fn is_text_anchor_for(&self, node_id: &NodeId) -> bool {
        match self {
            TextOrParagraphAnchor::TextAnchor(text_anchor) => text_anchor.at_node == *node_id,
            TextOrParagraphAnchor::ParagraphAnchor(_) => false,
        }
    }
}

#[derive(Clone, Debug)]
enum ParagraphInsertPosition {
    BeforeAnchor,
    EraseAnchorIfEmpty,
    AfterAnchor,
}

#[derive(Clone, Debug)]
struct ActionId {}

#[derive(Clone, Debug)]
enum Action {
    // subsumed by ParagraphInsert
    /*
    EmptyParagraphInsert {
        anchor: ParagraphId,
        first_paragraph: NewParagraph,
        // need additional ids for the other paragraphs
        additional_paragraphs: Vec<(ParagraphId, NewParagraph)>,
    },
    */
    // TODO: need concurrency-breakers (specify known range-style, formatting changes)
    Insert {
        // TODO: should this be a ParagraphOrTextAnchor? E.g. paste inserts?
        //       No, can just convert those inserts
        anchor: TextAnchor,
        before_paragraphs: Vec<PartiallyFormattedText>,
        paragraphs: Option<(Vec<NewParagraph>, ParagraphId, Vec<PartiallyFormattedText>)>,
        // this essentially splits the anchor paragraph
    },

    //TODO: can we somehow map this as an insert?
    ParagraphInsert {
        anchor: ParagraphId,
        position: ParagraphInsertPosition,
        // this paragraph gets the same id as this action
        first_paragraph: NewParagraph,
        // need additional ids for the other paragraphs
        additional_paragraphs: Vec<(ParagraphId, NewParagraph)>,
    },

    FormatChange {
        begin_anchor: TextAnchor,
        end_anchor: TextAnchor,
        format: Format,
    },

    ParagraphStyleChange {
        // must be 1 or more
        paragraphs: Vec<ParagraphId>,
        // the paragraph splices we know about in this range.
        known_paragraph_splices: Vec<ActionId>,
        //TODO: do we need all paragraphs to fall back in case there was splicing outside the range?
        // TODO: need something where we can select different styles
        paragraph_style: ParagraphStyle,
    },

    // Do we need a paragraph version of this?
    Erase {
        begin_anchor: TextAnchor,
        end_anchor: TextAnchor,
        // the splices we know about in this range.
        // If the nodes have been affected by another splice not in this list, that splice has won -> need to use the ids in the spliceinsert.
        known_splices: Vec<ActionId>,
        //TODO: erased content in case the anchors have been moved (which is detected by a splice insert not part of known_splices)
        //      node id, offset, text of all text nodes
        //      paragraphs: paragraph id, style, other properties
        //
        //TODO: need formatting for the before_paragraphs and the after_paragraphs in case it is pasted into an empty line.
        //      or maybe only for one of these if we want to keep the empty paragraph formatting (put it in the action at  splicing time)
    },

    SpliceInsert {
        anchor: TextAnchor,
        erase_id: ActionId,
        // used for the parts referenced in the erase where the splice was "won" by a concurrent other splice
        new_node_ids_if_necessary: Vec<NodeId>,
    },

    SpliceParagraphInsert {
        anchor: ParagraphId,
        position: ParagraphInsertPosition,
        erase_id: ActionId,
        // used for the parts referenced in the erase where the splice was "won" by a concurrent other splice
        new_node_ids_if_necessary: Vec<NodeId>,
        new_paragraph_style: ParagraphStyle,
    },

    UndoRedo {
        edit_id: ActionId,
        undo_counter_change: NonZeroI32,
    },
}

#[test]
fn node_id_order() {
    let n0 = NodeId {
        operation_id: 1,
        client_id: 3,
    };
    let n1 = NodeId {
        operation_id: 2,
        client_id: 1,
    };
    let n2 = NodeId {
        operation_id: 2,
        client_id: 2,
    };
    assert!(n0 < n1);
    assert!(n1 < n2);
}

#[derive(Debug)]
enum TextNode {
    FormatChange(TextFormatChange),

    Text {
        node: NodeId,
        offset: u32,
        offset_after: Option<u32>,
        text: String,
    },
    Tombstone {
        node: NodeId,
        offset: u32,
        offset_after: Option<u32>,
        length: u32,
    },
}

enum RelativePosition {
    Before,      // the offset asked for is before this node
    AtBeginning, // just at the beginning of this node
    Middle,      // ...
    AtEnd,
    After,
}

impl TextNode {
    // Returns the
    fn relative_positon(&self, offset: Option<u32>) -> RelativePosition {
        let (self_offset, self_offset_after, length) = match self {
            TextNode::Text {
                node: _,
                offset,
                offset_after,
                text,
            } => (offset, offset_after, text.len() as u32),
            Tombstone {
                node: _,
                offset,
                offset_after,
                length,
            } => (offset, offset_after, *length as u32),
            _ => panic!("should not call relative_positon "),
        };

        match offset {
            None => {
                if self_offset_after.is_none() {
                    RelativePosition::AtEnd
                } else {
                    RelativePosition::After
                }
            }
            Some(offset) => {
                if offset < *self_offset {
                    RelativePosition::Before
                } else if offset == *self_offset {
                    RelativePosition::AtBeginning
                } else {
                    //offset > *self_offset
                    if let Some(self_offset_after) = self_offset_after {
                        //TODO: check after
                        RelativePosition::Middle
                    } else {
                        // current goes all the way to the end, just need to figure out where that is
                        if offset < self_offset + length {
                            RelativePosition::Middle
                        } else {
                            RelativePosition::AtEnd
                        }
                    }
                }
            }
        }
    }

    fn contains(&self, anchor: &TextAnchor) -> bool {
        match self {
            TextNode::Text {
                node,
                offset,
                offset_after,
                text: _,
            }
            | TextNode::Tombstone {
                node,
                offset,
                offset_after,
                length: _,
            } if *node == anchor.at_node => match self.relative_positon(anchor.at_index) {
                RelativePosition::Before => false,
                RelativePosition::AtBeginning => true,
                RelativePosition::Middle => true,
                RelativePosition::AtEnd => true,
                RelativePosition::After => false,
            },
            _ => false,
        }
    }

    fn split_at(self, split_offset: u32) -> (Self, Self) {
        match self {
            TextNode::Text {
                node,
                offset: self_offset,
                offset_after,
                text,
            } => {
                let front_len = split_offset - self_offset;
                let (front_text, back_text) = text.split_at(front_len as usize);
                (
                    TextNode::Text {
                        node: node.clone(),
                        offset: self_offset,
                        offset_after: Some(split_offset),
                        text: front_text.to_string(),
                    },
                    TextNode::Text {
                        node,
                        offset: split_offset,
                        offset_after,
                        text: back_text.to_string(),
                    },
                )
            }
            Tombstone {
                node,
                offset: self_offset,
                offset_after,
                length,
            } => {
                let front_len = split_offset - self_offset;
                (
                    Tombstone {
                        node: node.clone(),
                        offset: self_offset,
                        offset_after: Some(split_offset as u32),
                        length: front_len as u32,
                    },
                    Tombstone {
                        node,
                        offset: split_offset as u32,
                        offset_after,
                        length: length - front_len,
                    },
                )
            }
            _ => panic!("cannot split this type of node {:?}", self),
        }
    }

    fn from_partially_formatted(partially_formatted: &PartiallyFormattedText) -> Vec<Self> {
        let mut result = vec![TextNode::Text {
            node: partially_formatted.node_id,
            offset: 0,
            offset_after: None,
            text: partially_formatted.text.clone(),
        }];
        if partially_formatted.format.values_to_set != 0 {
            //TODO: add formatting change nodes before & after (needs surrounding formatting as input)
        }
        result
    }
}

#[derive(Debug)]
struct Paragraph {
    paragraph_id: ParagraphId,
    contents: Vec<TextNode>,
}

impl Paragraph {
    fn from_new_paragraph(p: &NewParagraph) -> Self {
        Self {
            paragraph_id: p.node_id,
            contents: p
                .text
                .iter()
                .map(|frag| {
                    TextNode::Text {
                        node: frag.node_id,
                        // these nodes are always complete
                        offset: 0,
                        offset_after: None,
                        text: frag.text.clone(),
                    }
                })
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        for tn in &self.contents {
            match tn {
                TextNode::Text {
                    node,
                    offset,
                    offset_after,
                    text,
                } => return false,
                _ => {
                    // these nodes are all empty
                }
            }
        }
        true
    }

    fn to_tombstone(self) -> ParagraphTombstone {
        ParagraphTombstone {
            paragraph_id: self.paragraph_id,
            contents: self.contents,
        }
    }
}

#[derive(Debug)]
struct ParagraphTombstone {
    paragraph_id: ParagraphId,
    contents: Vec<TextNode>,
}

impl Paragraph {
    fn origin() -> Self {
        Self {
            paragraph_id: ParagraphId {
                operation_id: 0,
                client_id: 0,
            },
            contents: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum ParagraphNode {
    Paragraph(Paragraph),
    ParagraphTombstone(ParagraphTombstone),
}

impl ParagraphNode {
    fn paragraph_id(&self) -> &ParagraphId {
        match self {
            ParagraphNode::Paragraph(p) => &p.paragraph_id,
            ParagraphNode::ParagraphTombstone(pt) => &pt.paragraph_id,
        }
    }
    fn contents(&self) -> &Vec<TextNode> {
        match self {
            ParagraphNode::Paragraph(p) => &p.contents,
            ParagraphNode::ParagraphTombstone(pt) => &pt.contents,
        }
    }
    fn mut_contents(&mut self) -> &mut Vec<TextNode> {
        match self {
            ParagraphNode::Paragraph(p) => &mut p.contents,
            ParagraphNode::ParagraphTombstone(pt) => &mut pt.contents,
        }
    }
}

#[derive(Debug)]
struct RenderedFormattedText {
    node: NodeId,
    offset: u32,
    text: String,
    last_fragment: bool, // TODO: format
}

impl RenderedFormattedText {
    fn to_text(&self) -> String {
        self.text.to_string()
    }
}

#[derive(Debug)]
struct RenderedParagraph {
    paragraph_id: ParagraphId,
    content: Vec<RenderedFormattedText>,
}

impl RenderedParagraph {
    fn to_text(&self) -> String {
        self.content
            .iter()
            .map(|ft| ft.text.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug)]
struct RenderedDocument {
    paragraphs: Vec<RenderedParagraph>,
}

impl RenderedDocument {
    fn to_text(&self) -> String {
        self.paragraphs
            .iter()
            .map(|p| p.to_text())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug)]
struct DocumentState {
    // TODO: should it be possible to have formatting starts/ends between paragraphs?
    //       this can e.g. matter if user0 does a formatting between p0 and p1.
    //       If there is a concurrent (u0: insert of p2 after p0, u1: delete of p1), should it still have the formatting change?
    //            Yes, as u1 initially sees it with the formatting change
    //                 * have formatting nodes next to paragraphs
    //                 * keep formatting nodes in the tombstones
    //                      -> this also allows to restore them correctly
    //            should formatting changes generally be confined to paragraphs?
    //                 No, this is unnecessarily restrictive
    paragraphs: Vec<ParagraphNode>,
    client_selection: ClientSelection,
}

// Sample document:
// P0 { text_nodes: [TN0, TN1]}
// P1 { text_nodes: []}
// Iteration order:
// P0, TN0, TN1, P1
#[derive(Clone, Debug)]
struct DocumentStateIter<'a> {
    document_state: &'a DocumentState,
    paragraph_index: usize,
    text_node_index: Option<usize>, // if None, at the paragraph itself.
}

struct TextNodePosition {
    paragraph_index: usize,
    text_node_index: usize,
}

#[derive(Debug)]
enum ParagraphOrTextNode<'a> {
    Paragraph(&'a ParagraphNode),
    TextNode(&'a TextNode),
}

impl<'a> DocumentStateIter<'a> {
    fn current(&self) -> Option<ParagraphOrTextNode> {
        let p = self.document_state.paragraphs.get(self.paragraph_index)?;
        if let Some(text_node_index) = self.text_node_index {
            let text_node = p.contents().get(text_node_index)?;
            Some(ParagraphOrTextNode::TextNode(text_node))
        } else {
            Some(ParagraphOrTextNode::Paragraph(p))
        }
    }

    fn next_paragraph(&mut self) {
        self.paragraph_index += 1;
        self.text_node_index = None;
    }

    fn prev_paragraph(&mut self) {
        self.paragraph_index -= 1;
        self.text_node_index = self
            .document_state
            .paragraphs
            .get(self.paragraph_index)
            .map(|p| p.contents().len() - 1);
    }

    pub fn prev(&mut self) {
        let p = self.document_state.paragraphs.get(self.paragraph_index);
        if let Some(p) = p {
            let text_nodes_len = p.contents().len();
            if let Some(text_node_index) = &mut self.text_node_index {
                if *text_node_index - 1 >= 0 {
                    *text_node_index -= 1;
                } else {
                    // Move from text nodes to the paragraph
                    self.text_node_index = None;
                }
            } else {
                // at beginning of paragraph
                self.prev_paragraph();
            }
        }
    }

    pub fn next(&mut self) {
        let p = self.document_state.paragraphs.get(self.paragraph_index);
        if let Some(p) = p {
            let text_nodes_len = p.contents().len();
            if let Some(text_node_index) = &mut self.text_node_index {
                if *text_node_index + 1 < text_nodes_len {
                    *text_node_index += 1;
                } else {
                    self.next_paragraph();
                }
            } else if text_nodes_len != 0 {
                self.text_node_index = Some(0);
            } else {
                // empty paragraph
                self.next_paragraph();
            }
        }
    }

    pub fn skip_tombstone_incr(&mut self) {
        if let Some(ParagraphNode::ParagraphTombstone(_)) =
            self.document_state.paragraphs.get(self.paragraph_index)
        {
            self.next_paragraph();
            return self.skip_tombstone_incr();
        }
        match self.current() {
            Some(ParagraphOrTextNode::Paragraph(ParagraphNode::ParagraphTombstone(_)))
            | Some(ParagraphOrTextNode::TextNode(TextNode::Tombstone {
                node: _,
                offset: _,
                offset_after: _,
                length: _,
            })) => {
                self.next();
                return self.skip_tombstone_incr();
            }
            None => {}
            _ => {}
        }
    }

    pub fn skip_tombstone_decr(&mut self) {
        if let Some(ParagraphNode::ParagraphTombstone(_)) =
            self.document_state.paragraphs.get(self.paragraph_index)
        {
            self.prev_paragraph();
            return self.skip_tombstone_decr();
        }
        match self.current() {
            Some(ParagraphOrTextNode::Paragraph(ParagraphNode::ParagraphTombstone(_)))
            | Some(ParagraphOrTextNode::TextNode(TextNode::Tombstone {
                node: _,
                offset: _,
                offset_after: _,
                length: _,
            })) => {
                self.prev();
                return self.skip_tombstone_decr();
            }
            None => {}
            _ => {}
        }
    }
}

#[derive(Debug)]
struct DocumentStateMutIter<'a> {
    document_state: &'a mut DocumentState,
    paragraph_index: usize,
    text_node_index: Option<usize>,
}

#[derive(Debug)]
enum MutParagraphOrTextNode<'a> {
    Paragraph(&'a mut ParagraphNode),
    TextNode(&'a mut TextNode),
}

impl<'a> DocumentStateMutIter<'a> {
    fn current(&mut self) -> Option<MutParagraphOrTextNode> {
        let p = self
            .document_state
            .paragraphs
            .get_mut(self.paragraph_index)?;
        if let Some(text_node_index) = self.text_node_index {
            let text_node = p.mut_contents().get_mut(text_node_index)?;
            Some(MutParagraphOrTextNode::TextNode(text_node))
        } else {
            Some(MutParagraphOrTextNode::Paragraph(p))
        }
    }

    fn insert_into_paragraph_at(
        &mut self,
        paragraph_anchor_relativity: ParagraphAnchorRelativity,
        node_to_insert: TextNode,
    ) {
        println!("insert_into_paragraph_at self: {:?}", self);
        let p = self
            .document_state
            .paragraphs
            .get_mut(self.paragraph_index)
            .unwrap();
        match paragraph_anchor_relativity {
            ParagraphAnchorRelativity::AtBeginning => {
                p.mut_contents().insert(0, node_to_insert);
            }
            ParagraphAnchorRelativity::AtEnd => {
                p.mut_contents().push(node_to_insert);
            }
        }
    }

    // offset as measured from the beginning of the node, not the text nodej
    fn insert_into_current_text_node_at_offset(
        &mut self,
        offset: Option<u32>,
        node_to_insert: TextNode,
    ) {
        let p = self
            .document_state
            .paragraphs
            .get_mut(self.paragraph_index)
            .unwrap();
        let current_index = self.text_node_index.unwrap();
        let text_node = p.mut_contents().get_mut(current_index).unwrap();

        match text_node.relative_positon(offset) {
            RelativePosition::AtBeginning => {
                p.mut_contents().insert(current_index, node_to_insert);
            }
            RelativePosition::Middle => {
                let original = p.mut_contents().remove(current_index);
                let (before, after) = original.split_at(offset.unwrap());
                // NOTE: not super efficient, but not much is yet
                p.mut_contents().insert(current_index, after);
                p.mut_contents().insert(current_index, node_to_insert);
                p.mut_contents().insert(current_index, before);
            }
            RelativePosition::AtEnd => {
                p.mut_contents().insert(current_index + 1, node_to_insert);
            }
            RelativePosition::Before | RelativePosition::After => {
                panic!("cannot into something which does neither contain nor is near the position")
            }
        }
    }

    fn next_paragraph(&mut self) {
        self.paragraph_index += 1;
        self.text_node_index = None;
    }

    fn next(&mut self) {
        let p = self.document_state.paragraphs.get_mut(self.paragraph_index);
        if let Some(p) = p {
            let text_nodes_len = p.mut_contents().len();
            if let Some(text_node_index) = &mut self.text_node_index {
                if *text_node_index + 1 < text_nodes_len {
                    *text_node_index += 1;
                } else {
                    self.next_paragraph();
                }
            } else if text_nodes_len != 0 {
                self.text_node_index = Some(0);
            } else {
                // empty paragraph
                self.next_paragraph();
            }
        }
    }

    fn erase_current(&mut self) {
        if self.current().is_some() {
            if let Some(text_node_index) = self.text_node_index {
                // If there is no paragraph, but a text node, some invariants are no longer holding.
                let p = self
                    .document_state
                    .paragraphs
                    .get_mut(self.paragraph_index)
                    .unwrap();
                p.mut_contents().remove(text_node_index);
                if text_node_index >= p.mut_contents().len() {
                    self.next_paragraph()
                }
            } else if self
                .document_state
                .paragraphs
                .get(self.paragraph_index)
                .is_some()
            {
                self.document_state.paragraphs.remove(self.paragraph_index);
            }
        }
    }
}

impl DocumentState {
    fn empty() -> Self {
        Self {
            paragraphs: vec![ParagraphNode::Paragraph(Paragraph::origin())],
            client_selection: ClientSelection::NotSelected,
        }
    }

    fn mut_iter(&mut self) -> DocumentStateMutIter {
        DocumentStateMutIter {
            document_state: self,
            paragraph_index: 0,
            text_node_index: None,
        }
    }

    fn iter(&self) -> DocumentStateIter {
        DocumentStateIter {
            document_state: self,
            paragraph_index: 0,
            text_node_index: None,
        }
    }

    fn find_text_node(&self, node_id: &NodeId) -> Option<TextNodePosition> {
        let mut iter = self.iter();
        while let Some(entry) = iter.current() {
            match entry {
                ParagraphOrTextNode::Paragraph(_) => {}
                ParagraphOrTextNode::TextNode(tn) => match tn {
                    TextNode::Text {
                        node,
                        offset,
                        offset_after,
                        text: _,
                    }
                    | Tombstone {
                        node,
                        offset,
                        offset_after,
                        length: _,
                    } if node == node_id => {
                        return Some(TextNodePosition {
                            paragraph_index: iter.paragraph_index,
                            // Safe to unwrap, because we are in a text node -> must be set.
                            text_node_index: iter.text_node_index.unwrap(),
                        });
                    }
                    _ => {}
                },
            }
            iter.next();
        }
        None
    }

    fn get_non_tombstone_selection(&self) -> ClientSelection {
        //TODO: find existing node, first search left, then right
        match self.client_selection.clone() {
            ClientSelection::NotSelected => ClientSelection::NotSelected,
            ClientSelection::Caret(a) => self
                .find(&a)
                .and_then(|mut iter| {
                    // TODO!("Need to seek existing node, figure out real index")
                    iter.skip_tombstone_decr();
                    iter.current()
                        .and_then(|node| match (node, &a) {
                            (
                                ParagraphOrTextNode::Paragraph(ParagraphNode::Paragraph(
                                    Paragraph {
                                        paragraph_id,
                                        contents: _,
                                    },
                                )),
                                TextOrParagraphAnchor::ParagraphAnchor(a),
                            ) if a.paragraph_id == *paragraph_id => Some(ClientSelection::Caret(
                                TextOrParagraphAnchor::ParagraphAnchor(a.clone()),
                            )),
                            (
                                ParagraphOrTextNode::TextNode(TextNode::Text { node, .. }),
                                TextOrParagraphAnchor::TextAnchor(a),
                            ) if a.at_node == *node => {
                                Some(ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(
                                    //TODO: sanitize position
                                    a.clone(),
                                )))
                            }
                            (
                                ParagraphOrTextNode::Paragraph(ParagraphNode::Paragraph(
                                    Paragraph { paragraph_id, .. },
                                )),
                                _,
                            ) => {
                                Some(ClientSelection::Caret(
                                    TextOrParagraphAnchor::ParagraphAnchor(
                                        //TODO: sanitize position
                                        // TODO: probably want to go forward in some cases if possible (think e.g. paragraph being replaced)
                                        //       we should probably do this in all cases where the current paragraph disappears
                                        //       or maybe even create a tentative paragraph which gets created on keypress
                                        //         how would this interact with ctrl+x splicing?
                                        //           ideally, user0 can just keep typing while user1 cut & pastes the place where they are typing
                                        //           probably can do something with delaying update propagation for erases
                                        ParagraphAnchor {
                                            paragraph_id: paragraph_id.clone(),
                                            paragraph_anchor_relativity:
                                                ParagraphAnchorRelativity::AtEnd,
                                        },
                                    ),
                                ))
                            }
                            (
                                ParagraphOrTextNode::TextNode(TextNode::Text {
                                    node,
                                    offset_after,
                                    ..
                                }),
                                _,
                            ) => {
                                Some(ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(
                                    // TODO: should we move forward in some cases(see paragraph case comment)
                                    TextAnchor {
                                        at_node: node.clone(),
                                        at_index: offset_after.clone(), // position at end as this is a previous node
                                    },
                                )))
                            }
                            _ => None,
                        })
                        .or_else(|| {
                            iter.skip_tombstone_incr();
                            iter.current().and_then(|node| match (node, &a) {
                                (
                                    ParagraphOrTextNode::Paragraph(ParagraphNode::Paragraph(
                                        Paragraph { paragraph_id, .. },
                                    )),
                                    TextOrParagraphAnchor::ParagraphAnchor(a),
                                ) if a.paragraph_id == *paragraph_id => {
                                    Some(ClientSelection::Caret(
                                        TextOrParagraphAnchor::ParagraphAnchor(a.clone()),
                                    ))
                                }
                                (
                                    ParagraphOrTextNode::TextNode(TextNode::Text { node, .. }),
                                    TextOrParagraphAnchor::TextAnchor(a),
                                ) if a.at_node == *node => {
                                    Some(ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(
                                        //TODO: sanitize position
                                        a.clone(),
                                    )))
                                }
                                (
                                    ParagraphOrTextNode::Paragraph(ParagraphNode::Paragraph(
                                        Paragraph { paragraph_id, .. },
                                    )),
                                    _,
                                ) => Some(ClientSelection::Caret(
                                    TextOrParagraphAnchor::ParagraphAnchor(ParagraphAnchor {
                                        paragraph_id: paragraph_id.clone(),
                                        paragraph_anchor_relativity:
                                            ParagraphAnchorRelativity::AtEnd,
                                    }),
                                )),
                                (
                                    ParagraphOrTextNode::TextNode(TextNode::Text {
                                        node,
                                        offset_after,
                                        ..
                                    }),
                                    _,
                                ) => {
                                    Some(ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(
                                        TextAnchor {
                                            at_node: node.clone(),
                                            at_index: offset_after.clone(), // position at end as this is a previous node
                                        },
                                    )))
                                }
                                _ => None,
                            })
                        })
                })
                .unwrap_or(ClientSelection::Caret(
                    TextOrParagraphAnchor::ParagraphAnchor(ParagraphAnchor {
                        paragraph_id: ParagraphId {
                            operation_id: 0,
                            client_id: 0,
                        },
                        paragraph_anchor_relativity: ParagraphAnchorRelativity::AtBeginning,
                    }),
                )),
            ClientSelection::Range { .. } => todo!(),
        }
    }

    fn find<'a>(&'a self, anchor: &TextOrParagraphAnchor) -> Option<DocumentStateIter<'a>> {
        let mut iter = self.iter();
        while let Some(pos) = iter.current() {
            match (pos, &anchor) {
                (
                    ParagraphOrTextNode::Paragraph(ParagraphNode::Paragraph(Paragraph {
                        paragraph_id,
                        contents: _,
                    })),
                    TextOrParagraphAnchor::ParagraphAnchor(a),
                )
                | (
                    ParagraphOrTextNode::Paragraph(ParagraphNode::ParagraphTombstone(
                        ParagraphTombstone {
                            paragraph_id,
                            contents: _,
                        },
                    )),
                    TextOrParagraphAnchor::ParagraphAnchor(a),
                ) => {
                    if *paragraph_id == a.paragraph_id {
                        return Some(iter);
                    }
                }
                (ParagraphOrTextNode::TextNode(tn), TextOrParagraphAnchor::TextAnchor(a))
                    if tn.contains(a) =>
                {
                    return Some(iter);
                }
                _ => {}
            }
            iter.next();
        }
        None
    }

    fn find_mutable<'a>(&'a mut self, anchor: TextOrParagraphAnchor) -> DocumentStateMutIter<'a> {
        //todo!();
        self.mut_iter()
    }

    fn change_selection(&mut self, client_selection: ClientSelection) {
        self.client_selection = client_selection;
    }

    fn apply_operations(&mut self, ordered_ops: &BTreeMap<NodeId, Action>) {
        for op in ordered_ops {
            match op.1 {
                Action::ParagraphInsert {
                    anchor,
                    position,
                    first_paragraph,
                    additional_paragraphs,
                } => {
                    // find the insertion point
                    let iter = self.find_mutable(TextOrParagraphAnchor::ParagraphAnchor(
                        ParagraphAnchor {
                            paragraph_id: anchor.clone(),
                            paragraph_anchor_relativity: ParagraphAnchorRelativity::AtBeginning,
                        },
                    ));
                    let paragraph_index = iter.paragraph_index;
                    let maybe_paragraph = self.paragraphs.get(paragraph_index);
                    match position {
                        ParagraphInsertPosition::BeforeAnchor => todo!(),
                        ParagraphInsertPosition::EraseAnchorIfEmpty => {
                            // erase the current item if empty (replace with tombstone)
                            if let Some(p) = maybe_paragraph {
                                match p {
                                    ParagraphNode::Paragraph(p) => {
                                        if p.is_empty() {
                                            let p = self.paragraphs.remove(paragraph_index);
                                            if let ParagraphNode::Paragraph(p) = p {
                                                self.paragraphs.insert(
                                                    paragraph_index,
                                                    ParagraphNode::ParagraphTombstone(
                                                        p.to_tombstone(),
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                    ParagraphNode::ParagraphTombstone(_p) => {
                                        // already erased; nothing to do
                                    }
                                }
                                // insert the paragraph(s) after the current item
                                let paragraph = Paragraph::from_new_paragraph(first_paragraph);
                                if !additional_paragraphs.is_empty() {
                                    panic!("additional_paragraphs not supported yet")
                                }
                                self.paragraphs.insert(
                                    paragraph_index + 1,
                                    ParagraphNode::Paragraph(paragraph),
                                );
                            } else {
                                panic!("could not find paragraph")
                            }
                        }
                        ParagraphInsertPosition::AfterAnchor => todo!(),
                    }
                    // insert; splitting if necessary
                }
                Action::Insert {
                    anchor,
                    before_paragraphs,
                    paragraphs,
                } => {
                    let anchor_text_pos = self.find_text_node(&anchor.at_node).unwrap();
                    let p = self
                        .paragraphs
                        .get_mut(anchor_text_pos.paragraph_index)
                        .unwrap();

                    // Cases:
                    //     anchor at 0 -> no split necessary
                    //     anchor in middle
                    //     anchor at end -> no split necessary
                    //     no paragraphs in edit
                    //     paragraphs in edit
                    //         stuff before needs to be put in old paragraph
                    //         stuff after needs to be put in the new paragraph created after paragraphs
                    //
                    // Generalized:
                    // build before paragraphs (original content + maybe split node +  whatever is in the action)
                    //       optional: paragraphs from message
                    //       optional: after paragraphs (whatever is in the action + maybe split node + original content)
                    //
                    //
                    // finish the current node and return iterator/vector of what needs to go after or in a new paragraph

                    let after_anchor_leftover: Box<dyn Iterator<Item = TextNode>>;

                    if let Some(at_index) = anchor.at_index {
                        if at_index == 0 {
                            // before the node
                            let original =
                                p.mut_contents().split_off(anchor_text_pos.text_node_index);
                            after_anchor_leftover = Box::new(original.into_iter());
                            p.mut_contents().extend(before_paragraphs.iter().flat_map(
                                |partially_formatted| {
                                    TextNode::from_partially_formatted(partially_formatted)
                                },
                            ));
                        } else {
                            let mut original =
                                p.mut_contents().split_off(anchor_text_pos.text_node_index);
                            let original_after = original.split_off(1);
                            // Unwrap is ok because original has exactly 1 element.
                            let (before, after) = original.pop().unwrap().split_at(at_index);
                            let new_iter = std::iter::once(before).chain(
                                before_paragraphs
                                    .iter()
                                    .flat_map(TextNode::from_partially_formatted),
                            );
                            after_anchor_leftover =
                                Box::new(std::iter::once(after).chain(original_after.into_iter()));
                            p.mut_contents().extend(new_iter);
                        }
                    } else {
                        // after the node
                        let original = p
                            .mut_contents()
                            .split_off(anchor_text_pos.text_node_index + 1);
                        after_anchor_leftover = Box::new(original.into_iter());
                        p.mut_contents().extend(before_paragraphs.iter().flat_map(
                            |partially_formatted| {
                                TextNode::from_partially_formatted(partially_formatted)
                            },
                        ));
                    }
                    if let Some((paragraphs, after_paragraph_id, texts)) = paragraphs {
                        let trailing_paragraphs = self
                            .paragraphs
                            .split_off(anchor_text_pos.paragraph_index + 1);
                        // TODO: copy style, probably need to fix up some format start/end nodes
                        //       for formatting, need to end/start only the attributes which are changed for some of the inserted text
                        //       alternatively, could delay this to rendering time; however, slicing (and therefore erasing) still need materialization.
                        //       as erases are rare, just use the easy version for now.
                        //       -> only have format starts, no format ends
                        let new_after_paragraph = ParagraphNode::Paragraph(Paragraph {
                            paragraph_id: after_paragraph_id.clone(),
                            contents: texts
                                .iter()
                                .flat_map(TextNode::from_partially_formatted)
                                .chain(after_anchor_leftover)
                                .collect(),
                        });
                        self.paragraphs.extend(
                            paragraphs
                                .iter()
                                .map(|new_para| {
                                    ParagraphNode::Paragraph(Paragraph::from_new_paragraph(
                                        new_para,
                                    ))
                                })
                                .chain(std::iter::once(new_after_paragraph))
                                .chain(trailing_paragraphs.into_iter()),
                        );
                    } else {
                        p.mut_contents().extend(after_anchor_leftover);
                    }
                }

                Action::Erase {
                    begin_anchor,
                    end_anchor,
                    known_splices,
                } => todo!(),
                _ => todo!(),
            }
        }
    }

    fn render(&self) -> RenderedDocument {
        dbg!(self);
        // TODO: format cursor to render text
        RenderedDocument {
            paragraphs: self
                .paragraphs
                .iter()
                .filter_map(|p| {
                    if let ParagraphNode::Paragraph(p) = p {
                        Some(RenderedParagraph {
                            paragraph_id: p.paragraph_id,
                            content: p
                                .contents
                                .iter()
                                .filter_map(|tn: &TextNode| match tn {
                                    TextNode::FormatChange(_) => None,
                                    TextNode::Text {
                                        node,
                                        offset,
                                        offset_after,
                                        text,
                                    } => Some(RenderedFormattedText {
                                        node: node.clone(),
                                        offset: *offset,
                                        text: text.to_string(),
                                        last_fragment: offset_after.is_none(),
                                    }),
                                    //TODO: actually handle all cases here
                                    _ => None,
                                })
                                .collect(),
                        })
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
}

#[derive(Debug)]
struct Operations {
    ordered_ops: BTreeMap<NodeId, Action>,
}

impl Operations {
    fn empty() -> Self {
        Self {
            ordered_ops: Default::default(),
        }
    }

    fn add_or_replace_node(&mut self, node_id: NodeId, action: Action) {
        // TODO: better validation of legal options
        let _old_entry = self.ordered_ops.insert(node_id, action);
    }

    fn maximum_operation_id(&self) -> u64 {
        // TODO: match also inside actions; there are bigger ids there
        self.ordered_ops
            .iter()
            .map(|op| op.0.operation_id)
            .max()
            .unwrap_or_default()
    }
}

/*
struct DocumentState {
    // Need some random lookup into an ordered document; document should probably have backward/forward searchability
    // Store successor/predecessor in hashmap? Sounds slow
    // Custom structure with Next owning shared ptr, prev-non-owning
    // How do we get the head?
    // Initially can just go from any random node to the beginning
    //
    // have array pointing to rc-ed nodes
    // how do we find the entry in the array?
    // give a local id to each node based on the order, leave the least significant digits empty.
    // Insert between two nodes by taking the middle id between them, whenever we cannot give a unique one anymore,
    // redistribute locally (expand the range from the current node to both sides to make sure the fill-state is maximum x% in that space and distribute linearly), after a certain number of inserts completely re-id for even distribution.
    //
    // For now, brute force search for the pointer in the fragments.
    index: HashMap<NodeId, Rc<DocumentFragment>>,
    fragments: Vec<Rc<DocumentFragment>>,
    // TODO: Where do we track formatting changes?
    // have start & end of formatting markers
    // btrees for each start/end to find all applying ones?
    // Needs the local ids

    // Index from not needed new splice ids to the original node id.
    // NOTE: Can probably be optimized later to only contain things in case of undo operations
    splice_collisions_new_to_original: BTreeMap<NodeId, NodeId>,
}

struct OperationState {
    ordered_ops: BTreeMap<NodeId, Action>,
}

impl OperationState {
    // TODO: while rendering, keep an ordered vector/list of formatting changes (representing the render cursor)
    //TODO: render_formatted

    fn render_text(&self) -> String {
        "".to_string()
    }

    fn add_or_replace_node(&mut self, node_id: NodeId, action: Action) {
        // TODO: better validation of legal options
        let old_entry = self.ordered_ops.insert(node_id, action);
        match old_entry {
            Some(Action::Insert {
                at_node: _,
                at_index: _,
                text: _,
                is_into_empty_line: _,
            }) => {
                debug!(
                    "replaced {:?} with {:?}",
                    old_entry,
                    self.ordered_ops.get(&node_id)
                );
            }
            Some(old_action) => {
                error!(
                    "replaced {:?} with {:?}",
                    old_action,
                    self.ordered_ops.get(&node_id)
                );
            }
            _ => {}
        }
    }
}
*/

enum Input {
    Text(String),
    ParagraphBreak, // basically pressing ENTER
}

#[derive(Clone, Debug)]
enum ClientSelection {
    NotSelected,
    Caret(TextOrParagraphAnchor),
    Range {
        begin: TextOrParagraphAnchor,
        end: TextOrParagraphAnchor,
    },
}

#[derive(Debug)]
struct Client {
    id: NonZeroU64,
    document: DocumentState,
    operations: Operations,
    operation_counter: Option<u64>, // initially, just hold the one document, we'll extend this to hold snapshots and stuff soon enough
}

impl Client {
    // TODO: should load some existing document
    fn create(id: NonZeroU64) -> Self {
        Self {
            id,
            document: DocumentState::empty(),
            operations: Operations::empty(),
            operation_counter: None,
        }
    }

    fn change_selection(&mut self, client_selection: ClientSelection) {
        self.document.change_selection(client_selection);
        dbg!(self);
    }

    fn next_operation_id(&mut self) -> u64 {
        let new_value = std::cmp::max(
            self.operation_counter.unwrap_or_default(),
            self.operations.maximum_operation_id(),
        ) + 1;
        self.operation_counter = Some(new_value);
        new_value
    }

    fn new_node_id(&mut self) -> NodeId {
        NodeId {
            operation_id: self.next_operation_id(),
            client_id: self.id.get(),
        }
    }

    fn add_input(&mut self, input: Input) {
        let node_id;
        let operation;
        let new_caret;
        // TODO: use caret formatting if there is some (e.g. pressing ctrl+b with an empty selection)
        let format = TextFormatChange::default();
        match input {
            Input::Text(text) => match self.get_non_tombstone_selection() {
                ClientSelection::NotSelected => {
                    return;
                }
                ClientSelection::Caret(anchor) => match anchor {
                    TextOrParagraphAnchor::TextAnchor(a) => {
                        node_id = self.new_node_id();
                        operation = Action::Insert {
                            anchor: a,
                            // everything is in here, because we do not have new paragraphs in our input;
                            //    ENTER and PASTE is handled separately
                            before_paragraphs: vec![PartiallyFormattedText {
                                node_id,
                                text,
                                format,
                            }],
                            paragraphs: None,
                        };
                        new_caret = TextOrParagraphAnchor::TextAnchor(TextAnchor {
                            at_node: node_id,
                            at_index: None,
                        });
                    }
                    TextOrParagraphAnchor::ParagraphAnchor(anchor) => {
                        // This paragraph must be empty; otherwise a TextAnchor would have been returned
                        node_id = self.new_node_id();
                        operation = Action::ParagraphInsert {
                            anchor: anchor.paragraph_id,
                            position: ParagraphInsertPosition::EraseAnchorIfEmpty,
                            first_paragraph: NewParagraph {
                                node_id: ParagraphId::from_node_id(&node_id),
                                text: vec![PartiallyFormattedText {
                                    node_id,
                                    text,
                                    format,
                                }],
                            },
                            additional_paragraphs: Vec::new(),
                        };
                        new_caret = TextOrParagraphAnchor::TextAnchor(TextAnchor {
                            at_node: node_id,
                            at_index: None,
                        });
                    }
                },

                ClientSelection::Range { begin, end } => {
                    panic!("text inputs while a range selection is active are not supported yet")
                }
            },
            Input::ParagraphBreak => panic!("paragraphbreaks are not supported yet"),
        }
        self.operations.add_or_replace_node(node_id, operation);
        //TODO: this wipes the cursor, which is fine, but we need to set it again so the user can keep typing
        let mut new_document = DocumentState::empty();
        new_document.apply_operations(&self.operations.ordered_ops);
        self.change_selection(ClientSelection::Caret(new_caret));
        self.document = new_document;
        //TODO: generate operation
        //TODO: apply operation while updating cursors
        //      For this, get the document state, clear old cursors, add the cursors, apply the op, get the cursors
        // TODO: how do external (from other clients) inputs affect the cursor?
        //       E.g. what if a splice gets converted to a copy?
        //       DO NOT apply updates which change anything within the selected range, QUEUE them!
        //           range formats would generally be fine, but if the user e.g. erases the range, we do not want to insert new things before that
        //           to find out whether there are changes within the range, keep a copy of the document before, apply the changes
        //           and just iterate through the paragraphs/texts comparing the old document to the new one
        //       For simple carets, try to move it to the closest alias of the element (compare splice histories), or its tombstone
        //       For other user's carets, if there is a mismatch, just stop displaying until there is a new update.
    }

    fn get_non_tombstone_selection(&self) -> ClientSelection {
        self.document.get_non_tombstone_selection()
    }

    fn get_rendered_document(&self) -> RenderedDocument {
        // brute force for now
        let mut doc_state = DocumentState::empty();
        doc_state.apply_operations(&self.operations.ordered_ops);
        doc_state.render()
    }
}

fn main() {
    let doc = DocumentState::empty();
    println!("{:?}", doc);
    println!("{:?}", doc.render());
    println!("as text:\n{}", doc.render().to_text());
    let mut client2 = Client::create(NonZeroU64::new(2).unwrap());

    let mut client = Client::create(NonZeroU64::new(3).unwrap());
    let origin_caret =
        ClientSelection::Caret(TextOrParagraphAnchor::ParagraphAnchor(ParagraphAnchor {
            paragraph_id: ParagraphId {
                operation_id: 0,
                client_id: 0,
            },
            paragraph_anchor_relativity: ParagraphAnchorRelativity::AtBeginning,
        }));
    client.change_selection(origin_caret.clone());
    client2.change_selection(origin_caret);
    dbg!("{:?}", &client);
    dbg!("selection: {:?}", client.get_non_tombstone_selection());
    dbg!("client doc:\n{}", print(&client));
    client.add_input(Input::Text("test text".to_string()));
    dbg!("{:?}", &client);
    dbg!("selection: {:?}", client.get_non_tombstone_selection());
    dbg!("client doc:\n{}", print(&client));

    client2.add_input(Input::Text("client2's concurrent text".to_string()));
    let (node_id, action) = client2.operations.ordered_ops.iter().last().unwrap();
    client
        .operations
        .add_or_replace_node(node_id.clone(), action.clone());
    dbg!("{:?}", &client);
    dbg!("client doc:\n{}", print(&client));
    client.change_selection(ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(
        TextAnchor {
            at_node: NodeId {
                operation_id: 1,
                client_id: 3,
            },
            at_index: Some(4),
        },
    )));
    client.add_input(Input::Text("ed".to_string()));
    dbg!("{:?}", &client);
    dbg!("client doc:\n{}", print(&client));
    client.change_selection(ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(
        TextAnchor {
            at_node: NodeId {
                operation_id: 1,
                client_id: 3,
            },
            at_index: Some(4),
        },
    )));
    dbg!("client doc:\n{}", print(&client));
}

// TODO: test functionality:
// match against strings with formatting annotations (basically rich text) such as
// "/b" (bold) and "/B" (unbold) and cursor positions as '|'
// use forward slash for easier typing
// For this, first get the cursor position, then print everything to the string.
// keep track of the last previously printed fragment, to be able to print the escaped format changing characters
// only for the fragment with the cursor, we need to print that character in addition to the characters within the fragment.

// TODO: add printing state to detect reverse selection

enum Cursor {
    Caret(Option<u32>),
    RangeBegin(Option<u32>),
    RangeEnd(Option<u32>),
}

impl Cursor {
    fn get_offset(&self) -> Option<u32> {
        match self {
            Cursor::Caret(offset) | Cursor::RangeBegin(offset) | Cursor::RangeEnd(offset) => {
                offset.clone()
            }
        }
    }
}

#[derive(Default)]
struct RangePrinter {
    saw_caret: bool,
    saw_start: bool,
    saw_end: bool,
}

impl RangePrinter {
    fn print_start_range(&mut self) -> String {
        if !self.saw_start {
            self.saw_start = true;
            if self.saw_end { "]" } else { "[" }.to_string()
        } else {
            String::new()
        }
    }

    fn print_end_range(&mut self) -> String {
        if !self.saw_end {
            self.saw_end = true;
            "|".to_string()
        } else {
            String::new()
        }
    }

    fn print_caret(&mut self) -> String {
        if !self.saw_caret {
            self.saw_caret = true;
            "|".to_string()
        } else {
            String::new()
        }
    }

    fn print_cursor(&mut self, cursor: Cursor) -> String {
        match cursor {
            Cursor::Caret(_) => self.print_caret(),
            Cursor::RangeBegin(_) => self.print_start_range(),
            Cursor::RangeEnd(_) => self.print_end_range(),
        }
    }
}

fn print_text(
    p: &RenderedFormattedText,
    selection: &ClientSelection,
    rp: &mut RangePrinter,
) -> String {
    dbg!(p, selection);
    // TODO: formatting
    match selection {
        ClientSelection::Caret(TextOrParagraphAnchor::TextAnchor(a)) if a.at_node == p.node => {
            print_text_and_cursors(p, vec![Cursor::Caret(a.at_index)], rp)
        }
        ClientSelection::Range { begin, end }
            if begin.is_text_anchor_for(&p.node) || end.is_text_anchor_for(&p.node) =>
        {
            let mut cursor_positions = vec![];
            match begin {
                TextOrParagraphAnchor::TextAnchor(begin) if begin.at_node == p.node => {
                    cursor_positions.push(Cursor::RangeBegin(begin.at_index));
                }
                _ => {}
            };
            match end {
                TextOrParagraphAnchor::TextAnchor(end) if end.at_node == p.node => {
                    cursor_positions.push(Cursor::RangeEnd(end.at_index));
                }
                _ => {}
            };
            print_text_and_cursors(p, cursor_positions, rp)
        }
        _ => p.text.to_string(),
    }
}

fn print_text_and_cursors(
    p: &RenderedFormattedText,
    mut cursor_positions: Vec<Cursor>,
    rp: &mut RangePrinter,
) -> String {
    cursor_positions.sort_by(|left, right| {
        // Order None last
        if left.get_offset().is_some() && right.get_offset().is_none() {
            Ordering::Less
        } else if left.get_offset().is_none() && right.get_offset().is_some() {
            Ordering::Greater
        } else {
            left.get_offset().cmp(&right.get_offset())
        }
    });
    let mut result = String::new();
    let mut printed_so_far = p.offset;
    let print_remainder = |printed_so_far: u32, result: &mut String| {
        if (printed_so_far as usize) < (p.offset as usize + p.text.len()) {
            *result +=
                std::str::from_utf8(&p.text.as_bytes()[(printed_so_far - p.offset) as usize..])
                    .unwrap();
        }
    };
    for current_cursor in cursor_positions {
        if let Some(cursor_offset) = current_cursor.get_offset() {
            if printed_so_far < cursor_offset {
                let start_index = printed_so_far;
                let after_index = std::cmp::min((cursor_offset - p.offset) as usize, p.text.len());
                result +=
                    std::str::from_utf8(&p.text.as_bytes()[start_index as usize..after_index])
                        .unwrap();
                // TODO: check could this underflow?
                printed_so_far = (after_index as u32 - start_index) as u32;
            }
            if cursor_offset == printed_so_far {
                result += &rp.print_cursor(current_cursor);
            }
        } else {
            print_remainder(printed_so_far, &mut result);
            if p.last_fragment {
                result += &rp.print_cursor(current_cursor);
            }
        }
    }
    print_remainder(printed_so_far, &mut result);
    result
}

fn print_paragraph(
    p: &RenderedParagraph,
    selection: &ClientSelection,
    rp: &mut RangePrinter,
) -> String {
    let anchor_before = match selection {
        ClientSelection::Caret(TextOrParagraphAnchor::ParagraphAnchor(a))
            if a.paragraph_id == p.paragraph_id
                && a.paragraph_anchor_relativity == ParagraphAnchorRelativity::AtBeginning =>
        {
            "|".to_string()
        }
        ClientSelection::Range {
            begin: TextOrParagraphAnchor::ParagraphAnchor(begin),
            end: _,
        } if begin.paragraph_id == p.paragraph_id
            && begin.paragraph_anchor_relativity == ParagraphAnchorRelativity::AtBeginning =>
        {
            rp.print_start_range()
        }
        ClientSelection::Range {
            begin: _,
            end: TextOrParagraphAnchor::ParagraphAnchor(end),
        } if end.paragraph_id == p.paragraph_id
            && end.paragraph_anchor_relativity == ParagraphAnchorRelativity::AtBeginning =>
        {
            rp.print_end_range()
        }
        _ => "".to_string(),
    };
    let anchor_after = match selection {
        ClientSelection::Caret(TextOrParagraphAnchor::ParagraphAnchor(a))
            if a.paragraph_id == p.paragraph_id
                && a.paragraph_anchor_relativity == ParagraphAnchorRelativity::AtEnd =>
        {
            "|".to_string()
        }
        ClientSelection::Range {
            begin: TextOrParagraphAnchor::ParagraphAnchor(begin),
            end: _,
        } if begin.paragraph_id == p.paragraph_id
            && begin.paragraph_anchor_relativity == ParagraphAnchorRelativity::AtEnd =>
        {
            rp.print_start_range()
        }
        ClientSelection::Range {
            begin: _,
            end: TextOrParagraphAnchor::ParagraphAnchor(end),
        } if end.paragraph_id == p.paragraph_id
            && end.paragraph_anchor_relativity == ParagraphAnchorRelativity::AtEnd =>
        {
            rp.print_end_range()
        }
        _ => "".to_string(),
    };
    //TODO: print between and combine

    std::iter::once(anchor_before)
        .chain(p.content.iter().map(|text| print_text(text, selection, rp)))
        .chain(std::iter::once(anchor_after))
        .collect::<Vec<String>>()
        .join("")
}

fn print(client: &Client) -> String {
    let selection = client.get_non_tombstone_selection();
    dbg!(&selection);
    let doc = client.get_rendered_document();
    let mut rp = RangePrinter::default();
    doc.paragraphs
        .iter()
        .map(|p| print_paragraph(p, &selection, &mut rp))
        .collect::<Vec<String>>()
        .join("\r")
}

//TODO: make selection a separate thing; it does not really add anything to the document
//      even cursors from other users can be treated independently (if we want to propagate these into the backend at all)
//      update -immediately or on +get_selection?
//          caret: on update, search left for anchor, then right, if neither found, 0-paragraph
//          range: if one of the anchors cannot be found (only exists as tombstone): collapse to caret and end of range
