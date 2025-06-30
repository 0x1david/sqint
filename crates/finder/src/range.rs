use rangemap::RangeMap;

struct LineCol {
    line: usize,
    col: usize,
    byte_offset: usize,
}

struct Range {
    start: LineCol,
    end: LineCol,
}

struct ByteRange {
    start: usize,
    end: usize,
}

pub struct RangeFile<'a> {
    // Maps a byte range to the line number.
    map: RangeMap<usize, usize>,
    src: &'a str,
}

impl<'a> RangeFile<'a> {
    pub fn from_src(src: &'a str) -> RangeFile<'a> {
        let mut range_map = RangeMap::new();
        let mut line = 1;
        let mut last_line_start = 0;
        for (offset, ch) in src.char_indices() {
            if ch == '\n' {
                range_map.insert(last_line_start..offset, line);
                line += 1;
                last_line_start = offset + 1;
            }
        }

        range_map.insert(last_line_start..src.len(), line);

        Self {
            map: range_map,
            src,
        }
    }

    pub fn offset_to_linecol(&self, offset: usize) -> LineCol {
        let (range, line) = self
            .map
            .get_key_value(&offset)
            .expect("We analyze files right before calculating linecols.");
        let col = self.src[range.start..offset].chars().count() + 1;
        LineCol {
            col,
            line: *line,
            byte_offset: offset,
        }
    }

    pub fn range_to_linecols(&self, byte_range: ByteRange) -> Range {
        let (start_line_byte_range, start_line_number) = self
            .map
            .get_key_value(&byte_range.start)
            .expect("We analyze files right before calculating linecols.");
        let (end_line_byte_range, end_line_number) = self
            .map
            .get_key_value(&byte_range.end)
            .expect("We analyze files right before calculating linecols.");

        let start_col = self.src[start_line_byte_range.start..byte_range.start]
            .chars()
            .count()
            + 1;

        let end_col = if end_line_number == start_line_number {
            self.src[start_line_byte_range.start..byte_range.end]
                .chars()
                .count()
                + 1
        } else {
            self.src[end_line_byte_range.start..byte_range.end]
                .chars()
                .count()
                + 1
        };

        let start = LineCol {
            col: start_col,
            line: *start_line_number,
            byte_offset: byte_range.start,
        };
        let end = LineCol {
            col: end_col,
            line: *end_line_number,
            byte_offset: byte_range.end,
        };

        Range { start, end }
    }
}
