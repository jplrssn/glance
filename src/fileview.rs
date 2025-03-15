use memmap::Mmap;
use std::{cmp::max, fs::File, io::Error};

pub struct FileView {
    mmap: Mmap,

    pub num_lines: u64,
    pub max_num_cols: u64,

    // mapping of line no to byte position in mmap file
    line_to_byte_idx: Vec<u64>,
    line_to_num_cols: Vec<u64>,
}

impl FileView {
    pub fn open(filename: &str) -> Result<FileView, Error> {
        let file = File::open(filename)?;
        let mmap_open = unsafe { Mmap::map(&file) };
        match mmap_open {
            Ok(mmap) => Ok(FileView {
                mmap,
                num_lines: 0,
                max_num_cols: 0,
                line_to_byte_idx: vec![],
                line_to_num_cols: vec![],
            }),
            Err(e) => Err(e),
        }
    }

    pub fn build_linemap(&mut self) {
        let lines = self
            .mmap
            .split_inclusive(|i| match char::from_u32(u32::from(i.clone())) {
                Some(c) => c == '\n',
                None => false,
            });

        let mut bytes: u64 = 0;

        for line in lines {
            self.num_lines += 1;
            self.line_to_byte_idx.push(bytes);
            bytes += line.len() as u64;

            // TODO: Fix this for non-ASCII text
            let num_cols = line.len() as u64;
            self.line_to_num_cols.push(num_cols);
            self.max_num_cols = max(self.max_num_cols, num_cols);
        }
    }

    pub fn get_text(&self, line: u64, col_start: u64, col_end: u64) -> &str {
        use std::cmp::min;
        use std::str::from_utf8;

        let line_idx = line as usize;
        let byte_begin = (self.line_to_byte_idx[line_idx] + col_start) as usize;
        let num_cols = (min(self.line_to_num_cols[line_idx], col_end) - col_start) as usize;
        let byte_end = byte_begin + num_cols;

        from_utf8(&self.mmap[byte_begin..byte_end]).unwrap()
    }
}
