//! Abstractions that expose a simple interface for reading and storing tags according to some
//! underlying file format.
//!
//! The need for this abstraction arises from the differences that audiofiles have when storing
//! metadata. For example, MP3 uses a header for ID3v2, a trailer for ID3v1 while WAV has a special
//! "RIFF-chunk" which stores an ID3 tag.

use std::cmp;
use std::cmp::Ordering;
use std::fs;
use std::io::{self, Write};
use std::ops;

const COPY_BUF_SIZE: usize = 65536;

/// Refer to the module documentation.
pub trait Storage<'a> {
    type Reader: io::Read + io::Seek + 'a;
    type Writer: io::Write + io::Seek + 'a;

    /// Opens the storage for reading.
    fn reader(&'a mut self) -> io::Result<Self::Reader>;
    /// Opens the storage for writing.
    ///
    /// The written data is comitted to persistent storage when the
    /// writer is dropped, altough this will ignore any errors. The caller must manually commit by
    /// using `io::Write::flush` to check for errors.
    fn writer(&'a mut self) -> io::Result<Self::Writer>;
}

/// `PlainStorage` keeps track of a writeable region in a file and prevents accidental overwrites
/// of unrelated data. Any data following after the region is moved left and right as needed.
///
/// Padding is included from the reader.
#[derive(Debug)]
pub struct PlainStorage<F>
where
    F: StorageFile,
{
    /// The backing storage.
    file: F,
    /// The region that may be writen to including any padding.
    region: ops::Range<u64>,
}

/// This trait is the combination of the [`std::io`] stream traits with an additional method to resize the
/// file.
pub trait StorageFile: io::Read + io::Write + io::Seek + private::Sealed {
    /// Performs the resize. Assumes the same behaviour as [`std::fs::File::set_len`].
    fn set_len(&mut self, new_len: u64) -> io::Result<()>;
}

impl<'a, T> StorageFile for &'a mut T
where
    T: StorageFile,
{
    fn set_len(&mut self, new_len: u64) -> io::Result<()> {
        (*self).set_len(new_len)
    }
}

impl StorageFile for fs::File {
    fn set_len(&mut self, new_len: u64) -> io::Result<()> {
        fs::File::set_len(self, new_len)
    }
}

impl StorageFile for io::Cursor<Vec<u8>> {
    fn set_len(&mut self, new_len: u64) -> io::Result<()> {
        self.get_mut().resize(new_len as usize, 0);
        Ok(())
    }
}

impl<F> PlainStorage<F>
where
    F: StorageFile,
{
    /// Creates a new storage.
    pub fn new(file: F, region: ops::Range<u64>) -> PlainStorage<F> {
        PlainStorage { file, region }
    }
}

impl<'a, F> Storage<'a> for PlainStorage<F>
where
    F: StorageFile + 'a,
{
    type Reader = PlainReader<'a, F>;
    type Writer = PlainWriter<'a, F>;

    fn reader(&'a mut self) -> io::Result<Self::Reader> {
        self.file.seek(io::SeekFrom::Start(self.region.start))?;
        Ok(PlainReader::<'a, F> { storage: self })
    }

    fn writer(&'a mut self) -> io::Result<Self::Writer> {
        self.file.seek(io::SeekFrom::Start(self.region.start))?;
        Ok(PlainWriter::<'a, F> {
            storage: self,
            buffer: io::Cursor::new(Vec::new()),
            buffer_changed: true,
        })
    }
}

pub struct PlainReader<'a, F>
where
    F: StorageFile + 'a,
{
    storage: &'a mut PlainStorage<F>,
}

impl<'a, F> io::Read for PlainReader<'a, F>
where
    F: StorageFile,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let cur_pos = self.storage.file.stream_position()?;
        assert!(self.storage.region.start <= cur_pos);
        if self.storage.region.end <= cur_pos {
            return Ok(0);
        }
        let buf_upper_bound = cmp::min(
            buf.len(),
            cmp::max(self.storage.region.end - cur_pos, 0) as usize,
        );
        self.storage.file.read(&mut buf[0..buf_upper_bound])
    }
}

impl<'a, F> io::Seek for PlainReader<'a, F>
where
    F: StorageFile,
{
    fn seek(&mut self, rel_pos: io::SeekFrom) -> io::Result<u64> {
        let abs_cur_pos = self.storage.file.stream_position()?;
        let abs_pos = match rel_pos {
            io::SeekFrom::Start(i) => (self.storage.region.start + i) as i64,
            io::SeekFrom::End(i) => self.storage.region.end as i64 + i,
            io::SeekFrom::Current(i) => abs_cur_pos as i64 + i,
        };
        if abs_pos < self.storage.region.start as i64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "attempted to seek to before the start of the region",
            ));
        }
        let new_abs_pos = self
            .storage
            .file
            .seek(io::SeekFrom::Start(abs_pos as u64))?;
        Ok(new_abs_pos - self.storage.region.start)
    }
}

pub struct PlainWriter<'a, F>
where
    F: StorageFile + 'a,
{
    storage: &'a mut PlainStorage<F>,
    /// Data is writen to this buffer before it is committed to the underlying storage.
    buffer: io::Cursor<Vec<u8>>,
    /// A flag indicating that the buffer has been written to.
    buffer_changed: bool,
}

impl<'a, F> io::Write for PlainWriter<'a, F>
where
    F: StorageFile,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let nwritten = self.buffer.write(buf)?;
        self.buffer_changed = true;
        Ok(nwritten)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Check whether the buffer and file are out of sync.
        if !self.buffer_changed {
            return Ok(());
        }

        let buf_len = self.buffer.get_ref().len() as u64;
        fn range_len(r: &ops::Range<u64>) -> u64 {
            r.end - r.start
        }

        match buf_len.cmp(&range_len(&self.storage.region)) {
            Ordering::Greater => {
                // The region is not able to store the contents of the buffer. Grow it by moving the
                // following data to the end.
                let old_file_end = self.storage.file.seek(io::SeekFrom::End(0))?;
                let new_file_end = old_file_end + (buf_len - range_len(&self.storage.region));
                let old_region_end = self.storage.region.end;
                let new_region_end = self.storage.region.start + buf_len;

                self.storage.file.set_len(new_file_end)?;
                let mut rwbuf = [0; COPY_BUF_SIZE];
                let rwbuf_len = rwbuf.len();
                for i in 1.. {
                    let raw_from = old_file_end as i64 - i as i64 * rwbuf.len() as i64;
                    let raw_to = new_file_end.saturating_sub(i * rwbuf.len() as u64);
                    let from = cmp::max(old_region_end as i64, raw_from) as u64;
                    let to = cmp::max(new_region_end, raw_to);
                    assert!(from < to);

                    let diff = cmp::max(old_region_end as i64 - raw_from, 0) as usize;
                    let rwbuf_part = &mut rwbuf[cmp::min(diff, rwbuf_len)..];
                    self.storage.file.seek(io::SeekFrom::Start(from))?;
                    self.storage.file.read_exact(rwbuf_part)?;
                    self.storage.file.seek(io::SeekFrom::Start(to))?;
                    self.storage.file.write_all(rwbuf_part)?;
                    if rwbuf_part.len() < rwbuf_len {
                        break;
                    }
                }

                self.storage.region.end = new_region_end;
            }
            Ordering::Less => {
                // Shrink the file by moving the following data closer to the start.
                let old_file_end = self.storage.file.seek(io::SeekFrom::End(0))?;
                let old_region_end = self.storage.region.end;
                let new_region_end = self.storage.region.start + buf_len;
                let new_file_end = old_file_end - (old_region_end - new_region_end);

                let mut rwbuf = [0; COPY_BUF_SIZE];
                let rwbuf_len = rwbuf.len();
                for i in 0.. {
                    let from = old_region_end + i * rwbuf.len() as u64;
                    let to = new_region_end + i * rwbuf.len() as u64;
                    assert!(from >= to);

                    let part = (to + rwbuf_len as u64).saturating_sub(new_file_end);
                    let rwbuf_part = &mut rwbuf[part as usize..];
                    self.storage.file.seek(io::SeekFrom::Start(from))?;
                    self.storage.file.read_exact(rwbuf_part)?;
                    self.storage.file.seek(io::SeekFrom::Start(to))?;
                    self.storage.file.write_all(rwbuf_part)?;
                    if rwbuf_part.len() < rwbuf_len {
                        break;
                    }
                }

                self.storage.file.set_len(new_file_end)?;
                self.storage.region.end = new_region_end;
            }
            Ordering::Equal => {}
        }

        assert!(buf_len <= range_len(&self.storage.region));
        // Okay, it's safe to commit our buffer to disk now.
        self.storage
            .file
            .seek(io::SeekFrom::Start(self.storage.region.start))?;
        self.storage.file.write_all(&self.buffer.get_ref()[..])?;
        self.storage.file.flush()?;
        self.buffer_changed = false;
        Ok(())
    }
}

impl<'a, F> io::Seek for PlainWriter<'a, F>
where
    F: StorageFile,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.buffer.seek(pos)
    }
}

impl<'a, F> Drop for PlainWriter<'a, F>
where
    F: StorageFile,
{
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

// https://rust-lang.github.io/api-guidelines/future-proofing.html#c-sealed
mod private {
    pub trait Sealed {}

    impl<'a, T: Sealed> Sealed for &'a mut T {}
    impl Sealed for std::fs::File {}
    impl Sealed for std::io::Cursor<Vec<u8>> {}
}
