## Piece table implementation

A piece table is a datastructure used to maintain the sequence of character representing the current state of a file
being edited.

It stores the original text as a immutable buffer (*the file buffer*) and every modification is appended to a second 
buffer called (the *add buffer*) which grow without bound and is append-only.

- A delete is handled by splitting a piece into two pieces****
- A special caseo ccurs when the deleted item is at the b eginning or end of the piece in which case we simply adjust
  the pointer or the piece length
- An insert is handled by splitting the piece into three pieces

## Reusable edits
