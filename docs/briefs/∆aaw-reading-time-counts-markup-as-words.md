# `readingTime` counts markup as words

## Background

`readingTime` counts markup as words — `words()` in `crates/lemmon-vm/src/builtins/ text.rs`
counts a code block's text like prose and an entity's name as a word (its own test asserts
`a &gt; b` is three). Decide whether code counts at all or by an estimate from line length;
`∆hyz` allowlists what then differs. (The `syn-` span breaks D40 added go with `∆7wK`: a
block's default content carries none.)
