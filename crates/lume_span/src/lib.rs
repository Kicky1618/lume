use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: usize,
    pub path: PathBuf,
    pub source: String,
}

impl SourceFile {
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (idx, ch) in self.source.char_indices() {
            if idx >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    pub fn line_text(&self, line_number: usize) -> Option<&str> {
        self.source.lines().nth(line_number.saturating_sub(1))
    }
}

#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add_file(&mut self, path: PathBuf, source: String) -> usize {
        let id = self.files.len();
        self.files.push(SourceFile { id, path, source });
        id
    }

    pub fn get(&self, id: usize) -> Option<&SourceFile> {
        self.files.get(id)
    }
}
