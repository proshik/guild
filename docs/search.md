# Search and replacement

Cmd+F opens local Find. Cmd+R opens local Replace in editable files. Cmd+Shift+F/R
open Find/Replace in Files from a project editor, Explorer, terminal or dialog.
Double Shift opens Search Everywhere; an empty query starts with recent files.
Non-project filtering is available for All, Files and Symbols. Text searches the
project; Actions has no preview or non-project filter to pretend otherwise.
Run and Reload File remain available as actions without default shortcuts.

Local fields support case, whole words, regex, multiline queries and searching in
the original selection, tracked through typing, local/project replacement and
Undo/Redo. Reload refreshes matches; a restored file gets a search bound to its
current editor. Closed search highlights stay cleared when the theme changes.
The history button and Option+Up/Down recall searches.
Enter/Shift+Enter navigate matches. Exclude skips the current local match; project
Exclude/Include File excludes that canonical document from the current popup.
Terminal output and dependency sources remain read-only.

Regex uses Rust's grep engine, not Java's regex dialect: lookarounds and pattern
backreferences are unsupported. Replacements expand `$1`, `${name}` and `$$`;
literal replacement leaves dollar signs literal. Preserve case applies to literal
search (upper, lower and title case), and is unavailable for regex replacement.

Project replacement searches full current buffers, including unsaved text, and
validates disk snapshots before editing. It edits through the editor, with one
Undo per file, preserving preceding user edits and following the existing save
policy. With autosave off, changes remain unsaved until normal Save. There is no
separate direct-disk replacement writer, undo journal or whole-project Undo.
Changed or unsafe files are refused with their reason and actual applied counts.
Watcher refresh waits for an active replacement to finish. Cancelling preserves
completed edits and reports the number of unprocessed files left unchanged.

Binary/unsupported encodings, files above the existing 20 MiB editor open limit,
non-roundtrippable mixed line endings and outside-project links are not bulk-edit
candidates. UTF-8 BOM, uniform LF/CRLF, existing permissions and no-final-newline
files use the existing reader/save path. Different dirty buffers opened through
aliases are checked again immediately before each edit and refused rather than
choosing an implicit overwrite winner, including aliases opened after searching or through another open project. Alias
paths are resolved in the background. If open file paths or editor identities
change while validation is pending, that edit is refused; search again.

Search results retain the existing 100 rows/file and 5000 total limits. Retained
full snapshots and prepared replacement output are each bounded at 64 MiB, with
document/project occurrence plans bounded at 100000 matches.
Find and navigation do not validate a hypothetical replacement of every match.
The output limit applies to the selected Single/Replace All plan.
When a limit is reached, narrow the query: Replace All does not silently operate
on a truncated result set. The ordinary filesystem save policy is not a transaction
with other applications.

## Recent Files

Recent Files (Cmd+E) is a selection-only project MRU: a named header, the
existing edited-only checkbox, compact file rows and keyboard/click selection.
It has no search field. Open/reopen order is recorded immediately and paths are
deduplicated. Arrows and Enter choose, Escape dismisses, and Backspace uses the
ordinary guarded file close; cancelling preserves the recent entry. Printable
keys cannot reach the editor behind the popup. Recent Locations retains its
query input. Intended query autofocus starts the real blinking component caret;
passive optional fields keep their muted hints until explicitly focused.
