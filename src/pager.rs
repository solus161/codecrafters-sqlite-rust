use std::fmt::Debug;
use std::fs::File;
use std::os::unix::fs::FileExt;

use crate::exceptions::CustomErr;

pub struct Pager {
    file: File,
    page_size: u64,
    page_count: u64, // cache: HashMap<usize, Vec<u8>>
}

impl Pager {
    pub fn new(file_path: &str) -> Self {
        let file_obj = File::open(file_path).expect("Invalid file path");

        // Read first 100 bytes to extract metadata
        let offset = 0u64;
        let mut buf = vec![0u8; 100];
        file_obj
            .read_exact_at(&mut buf, offset)
            .expect("Error reading first 100 bytes");

        // 2 byte offset 16, page size.
        // Per the SQLite spec this is a power of two between 512 and 32768,
        // or the value 1 meaning a page size of 65536 (which doesn't fit in u16).
        let raw_page_size = u16::from_be_bytes(buf[16..16 + 2].try_into().unwrap()) as u64;
        let page_size = if raw_page_size == 1 {
            65536
        } else {
            raw_page_size
        };

        // Page count
        let page_count = u32::from_be_bytes(buf[16 + 12..16 + 12 + 4].try_into().unwrap()) as u64;

        Self {
            file: file_obj,
            page_size,
            page_count,
        }
    }

    pub fn page_size(&self) -> u64 {
        self.page_size
    }

    pub fn page_count(&self) -> u64 {
        self.page_count
    }

    pub fn read_page(&self, page_number: &u64) -> Result<Vec<u8>, CustomErr> {
        let offset = (page_number - 1) * self.page_size;
        let mut buf = vec![0u8; self.page_size as usize];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(buf)
    }
}

impl Debug for Pager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pager")
            .field("page_size", &self.page_size)
            .field("page_count", &self.page_count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::pager::Pager;

    #[test]
    fn test_pager_load_file() {
        let pager = Pager::new("companies.db");
        assert_eq!(pager.page_size(), 4096 as u64);
        assert_eq!(pager.page_count(), 1910 as u64);
    }
}
