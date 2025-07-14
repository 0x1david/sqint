use std::fmt::Display;

use rangemap::RangeMap;
use rustpython_parser::text_size::TextRange;
use std::collections::HashSet;

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct LineCol {
    line: usize,
    col: usize,
    byte_offset: usize,
}

impl Display for LineCol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:", self.line, self.col)
    }
}

#[derive(Debug, Clone)]
pub struct Range {
    pub start: LineCol,
}

#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    start: usize,
}

impl From<TextRange> for ByteRange {
    fn from(value: TextRange) -> Self {
        Self {
            start: value.start().to_usize(),
        }
    }
}

#[derive(Debug, Clone)]
struct PragmaMap {
    // Maps filename to a set of lines to ignore from the analysis
    ignores: HashSet<usize>,
}

impl PragmaMap {
    fn new() -> Self {
        Self {
            ignores: HashSet::new(),
        }
    }

    fn add_ignore(&mut self, line: usize) {
        self.ignores.insert(line);
    }

    pub fn should_ignore_line(&self, line: usize) -> bool {
        self.ignores.contains(&line)
    }
}

#[derive(Debug, Clone)]
pub struct PreanalyzedFile<'a> {
    // Maps a byte range to the line number.
    map: RangeMap<usize, usize>,
    src: &'a str,
    pragmas: PragmaMap,
}

impl<'a> PreanalyzedFile<'a> {
    pub fn should_ignore_stmt_at(&self, offset: usize) -> bool {
        let line = self
            .map
            .get(&offset)
            .expect("Shouldn't ever exceed indexed lines");
        self.pragmas.should_ignore_line(*line)
    }
    pub fn from_src(src: &'a str) -> Self {
        let mut range_map = RangeMap::new();
        let mut pragmas = PragmaMap::new();
        let mut line = 1;
        let mut last_line_start = 0;

        // Process line by line to properly handle pragmas
        for (offset, ch) in src.char_indices() {
            if ch == '\n' {
                // Check current line for pragma
                let line_text = &src[last_line_start..offset];
                if Self::line_has_pragma(line_text) {
                    pragmas.add_ignore(line);
                }

                range_map.insert(last_line_start..(offset + 1), line);
                line += 1;
                last_line_start = offset + 1;
            }
        }

        // Handle last line if no trailing newline
        if last_line_start < src.len() {
            let line_text = &src[last_line_start..];
            if Self::line_has_pragma(line_text) {
                pragmas.add_ignore(line);
            }
        }

        range_map.insert(last_line_start..src.len() + 1, line);

        Self {
            map: range_map,
            src,
            pragmas,
        }
    }

    fn line_has_pragma(line: &str) -> bool {
        // Look for comment and check if it contains sqint: ignore
        if let Some(comment_pos) = line.find('#') {
            let comment = &line[comment_pos + 1..].trim();
            return comment.starts_with("sqint: ignore")
                || comment.starts_with("sqint:ignore")
                || comment.contains("sqint: ignore");
        }
        false
    }

    pub fn byterange_to_range(&self, byte_range: ByteRange) -> Range {
        let (start_line_byte_range, start_line_number) = self
            .map
            .get_key_value(&byte_range.start)
            .expect("We analyze files right before calculating linecols.");

        let start_col = self.src[start_line_byte_range.start..byte_range.start]
            .chars()
            .count()
            + 1;

        let start = LineCol {
            col: start_col,
            line: *start_line_number,
            byte_offset: byte_range.start,
        };

        Range { start }
    }
}
