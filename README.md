# crdt_splice
A Rust splicing collaborative editor with formatting support is under construction here, with lots of experimentation as (to the author's knowledge) nothing like this has ever been done.

**None of the interactive parts are working so far**

## Document model
A document contains paragraphs which contain text. Paragraphs are separated with `\r` characters.

Text can have text formatting such as bold, italic, color, font, ....

Each paragraph can be formatted to be e.g. a heading, a list item, ... and have additional properties such as indentation.

## Collaborative implementation
crdt_splice uses an [OpSets](https://arxiv.org/abs/1805.04263) CRDT approach to get a total order on operations.

Inserts reference the operation and the text offset within that operation.

To enable splicing, any erased text can be reinserted and the identity will be kept.
If a text in spliced several times, the first one keeps the original identity, while later "splicings" get a copy of what they spliced.
For this to work, erases need to store copies of all text fragments erased at the time the erase is generated.
This will also allow future versions to detect concurrent edits better and provide more uniform behavior.

Undos are handled with a counter on each operation which is initially `0` and increased if the operation is undone and decreased if it is redone.
If the undo counter is `> 0`, the operation is treated as undone.
This leads to a natural voting behavior; if an operation is concurrently undone in two places and redone in one, it stays undone.
To avoid undos/redos not doing anything, a client will always increment/decrement just enough to make the counter reach `1` or `0`.

### Client interface
The frontend on the client sends all changes to the backend on the client which provides the updates to the frontend.

```
insert(into_node, at_index) -> changeset

erase(erase_start_node, erase_start_at_index, erase_end_node, erase_end_at_index) -> changeset

format(format_start_node, format_start_at_index, format_end_node, format_end_at_index, formatting_description) -> changeset
```
