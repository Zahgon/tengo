//! Source positions and file sets (parser/pos.go, parser/source_file.go).

use std::cell::RefCell;
use std::rc::Rc;

/// A position in a file set; 0 is NoPos.
pub type Pos = i64;
pub const NO_POS: Pos = 0;

#[derive(Debug, Clone, Default)]
pub struct SourceFilePos {
    pub filename: String,
    pub offset: i64,
    pub line: i64,
    pub column: i64,
}

impl SourceFilePos {
    pub fn is_valid(&self) -> bool {
        self.line > 0
    }
}

impl std::fmt::Display for SourceFilePos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = self.filename.clone();
        if self.is_valid() {
            if !s.is_empty() {
                s.push(':');
            }
            s.push_str(&self.line.to_string());
            if self.column != 0 {
                s.push(':');
                s.push_str(&self.column.to_string());
            }
        }
        if s.is_empty() {
            s.push('-');
        }
        f.write_str(&s)
    }
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub name: String,
    pub base: i64,
    pub size: i64,
    pub lines: Vec<i64>,
}

impl SourceFile {
    fn unpack(&self, offset: i64) -> (String, i64, i64) {
        let mut line = 0;
        let mut column = 0;
        // searchInts: index of the last line start <= offset
        let mut i = 0usize;
        let mut j = self.lines.len();
        while i < j {
            let h = i + (j - i) / 2;
            if self.lines[h] <= offset {
                i = h + 1;
            } else {
                j = h;
            }
        }
        let idx = i as i64 - 1;
        if idx >= 0 {
            line = idx + 1;
            column = offset - self.lines[idx as usize] + 1;
        }
        (self.name.clone(), line, column)
    }

    pub fn position(&self, p: Pos) -> SourceFilePos {
        let offset = p - self.base;
        let (filename, line, column) = self.unpack(offset);
        SourceFilePos { filename, offset, line, column }
    }

    pub fn add_line(&mut self, offset: i64) {
        let i = self.lines.len();
        if (i == 0 || self.lines[i - 1] < offset) && offset < self.size {
            self.lines.push(offset);
        }
    }
}

#[derive(Debug, Default)]
pub struct SourceFileSet {
    pub base: i64,
    pub files: Vec<SourceFile>,
    last: Option<usize>,
}

pub type FileSetRef = Rc<RefCell<SourceFileSet>>;

impl SourceFileSet {
    pub fn new() -> FileSetRef {
        Rc::new(RefCell::new(SourceFileSet { base: 1, files: Vec::new(), last: None }))
    }

    /// AddFile: returns the index of the new file.
    pub fn add_file(&mut self, filename: &str, base: i64, size: i64) -> usize {
        let base = if base < 0 { self.base } else { base };
        self.files.push(SourceFile { name: filename.to_string(), base, size, lines: vec![0] });
        self.base = base + size + 1;
        let idx = self.files.len() - 1;
        self.last = Some(idx);
        idx
    }

    fn file_index(&mut self, p: Pos) -> Option<usize> {
        if let Some(l) = self.last {
            let f = &self.files[l];
            if f.base <= p && p <= f.base + f.size {
                return Some(l);
            }
        }
        // searchFiles: last file with base <= p
        let mut i = 0usize;
        let mut j = self.files.len();
        while i < j {
            let h = i + (j - i) / 2;
            if self.files[h].base > p {
                j = h;
            } else {
                i = h + 1;
            }
        }
        let idx = i as i64 - 1;
        if idx >= 0 {
            let f = &self.files[idx as usize];
            if p <= f.base + f.size {
                self.last = Some(idx as usize);
                return Some(idx as usize);
            }
        }
        None
    }

    pub fn position(&mut self, p: Pos) -> SourceFilePos {
        if p != NO_POS {
            if let Some(i) = self.file_index(p) {
                return self.files[i].position(p);
            }
        }
        SourceFilePos::default()
    }
}
