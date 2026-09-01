# deslop

Terminal tool for rewriting markdown (for example AI generated content) in your
own words, one paragraph at a time.

## Usage

```
deslop <input.md> [-o output.md]
```

Default output is `<input stem>.rewritten.md` next to the input. The input file
is never modified.

## Flow

1. The file is split into chunks: paragraphs separated by blank lines. Fenced
   code blocks stay whole.
2. For each chunk you see three panes: the original text, a live word-level
   diff (red struck-through = removed, green = added), and an editor where you
   write your own version.
3. When all chunks are committed you get a full-document preview for last
   minute edits, then save.

## Keys

| Key | In chunk view | In preview |
| --- | --- | --- |
| Ctrl+S | Commit chunk, go to next | Save file and exit |
| Ctrl+K | Keep original as-is, go to next | |
| Ctrl+O | Copy original into the editor | |
| Ctrl+P | Back to previous chunk | Back to chunks (discards preview edits) |
| Ctrl+Q | Quit without saving (asks y/n) | Same |

Committing an empty editor drops that paragraph from the output.

## Build

```
cargo build --release
```
