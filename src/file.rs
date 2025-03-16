use memmap::Mmap;
use simdutf8::basic::from_utf8;
use std::sync::{Arc, Mutex};
use std::{cmp::max, io::Error};

pub struct File {
    mmap: Mmap,
}

pub type FilePtr = Arc<File>;

impl File {
    pub fn open(filename: &str) -> Result<FilePtr, Error> {
        let file = std::fs::File::open(filename)?;
        let mmap_open = unsafe { Mmap::map(&file) };
        match mmap_open {
            Ok(mmap) => Ok(Arc::new(File { mmap })),
            Err(e) => Err(e),
        }
    }

    pub fn build_linemap(&self, metadata: &MetadataPtr) {
        let lines = self
            .mmap
            .split_inclusive(|i| match char::from_u32(u32::from(i.clone())) {
                Some(c) => c == '\n',
                None => false,
            });

        let mut total_bytes: u64 = 0;

        for line in lines {
            let mut metadata = metadata.lock().unwrap();
            metadata.num_lines += 1;
            metadata.line_to_byte_idx.push(total_bytes);

            let num_bytes = line.len() as u64;
            metadata.line_to_num_bytes.push(num_bytes);
            total_bytes += num_bytes;

            let chars = from_utf8(line).unwrap();
            let mut num_cols: u64 = 0;
            for _ in chars.chars() {
                num_cols += 1;
            }

            metadata.line_to_num_cols.push(num_cols);
            metadata.max_num_cols = max(metadata.max_num_cols, num_cols);
        }
    }

    fn cols_to_bytes(s: &str, col_start: usize, col_end: usize) -> (usize, usize) {
        let mut start: usize = s.len();
        let mut end: usize = s.len();
        let mut col = 0;
        for (pos, _) in s.char_indices() {
            if col == col_start {
                start = pos;
            }
            if col == col_end {
                end = pos;
            }
            col += 1;
        }
        (start, end)
    }

    pub fn get_text(&self, metadata: &Metadata, line: u64, col_start: u64, col_end: u64) -> &str {
        use std::cmp::min;

        let line_idx = line as usize;
        let byte_begin = metadata.line_to_byte_idx[line_idx] as usize;
        let byte_end = byte_begin + metadata.line_to_num_bytes[line_idx] as usize;

        let chars = from_utf8(&self.mmap[byte_begin..byte_end]).unwrap();
        let col_end = min(col_end as usize, chars.len());
        let col_start = min(col_start as usize, col_end);

        let (slice_start, slice_end) = Self::cols_to_bytes(chars, col_start, col_end);
        &chars[slice_start..slice_end]
    }
}

pub struct Metadata {
    pub num_lines: u64,
    pub max_num_cols: u64,

    // mapping of line no to byte position in mmap file
    line_to_byte_idx: Vec<u64>,
    line_to_num_bytes: Vec<u64>,
    line_to_num_cols: Vec<u64>,
}

pub type MetadataPtr = Arc<Mutex<Metadata>>;

impl Metadata {
    pub fn new() -> MetadataPtr {
        let m = Metadata {
            num_lines: 0,
            max_num_cols: 0,
            line_to_byte_idx: vec![],
            line_to_num_bytes: vec![],
            line_to_num_cols: vec![],
        };
        Arc::new(Mutex::new(m))
    }
}
