use memmap::Mmap;
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
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

        let mut bytes: u64 = 0;

        for line in lines {
            let mut metadata = metadata.lock().unwrap();
            metadata.num_lines += 1;
            metadata.line_to_byte_idx.push(bytes);
            bytes += line.len() as u64;

            // TODO: Fix this for non-ASCII text
            let num_cols = line.len() as u64;
            metadata.line_to_num_cols.push(num_cols);
            metadata.max_num_cols = max(metadata.max_num_cols, num_cols);
        }
    }

    pub fn get_text(&self, metadata: &Metadata, line: u64, col_start: u64, col_end: u64) -> &str {
        use std::cmp::min;
        use std::str::from_utf8;

        let line_idx = line as usize;
        let byte_begin = (metadata.line_to_byte_idx[line_idx] + col_start) as usize;
        let num_cols = (min(metadata.line_to_num_cols[line_idx], col_end) - col_start) as usize;
        let byte_end = byte_begin + num_cols;

        from_utf8(&self.mmap[byte_begin..byte_end]).unwrap()
    }
}

pub struct Metadata {
    pub num_lines: u64,
    pub max_num_cols: u64,

    // mapping of line no to byte position in mmap file
    line_to_byte_idx: Vec<u64>,
    line_to_num_cols: Vec<u64>,
}

pub type MetadataPtr = Arc<Mutex<Metadata>>;

impl Metadata {
    pub fn new() -> MetadataPtr {
        let m = Metadata {
            num_lines: 0,
            max_num_cols: 0,
            line_to_byte_idx: vec![],
            line_to_num_cols: vec![],
        };
        Arc::new(Mutex::new(m))
    }
}
