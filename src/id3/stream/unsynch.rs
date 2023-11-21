//! The only purpose of unsynchronisation is to make the ID3v2 tag as compatible as possible with
//! existing software and hardware. There is no use in 'unsynchronising' tags if the file is only
//! to be processed only by ID3v2 aware software and hardware. Unsynchronisation is only useful
//! with tags in MPEG 1/2 layer I, II and III, MPEG 2.5 and AAC files.
use std::io;

/// Returns the synchsafe variant of a `u32` value.
pub fn encode_u32(n: u32) -> u32 {
    assert!(n < 0x1000_0000);
    let mut x: u32 = n & 0x7F | (n & 0xFFFF_FF80) << 1;
    x = x & 0x7FFF | (x & 0xFFFF_8000) << 1;
    x = x & 0x7F_FFFF | (x & 0xFF80_0000) << 1;
    x
}

/// Returns the unsynchsafe varaiant of a `u32` value.
pub fn decode_u32(n: u32) -> u32 {
    n & 0xFF | (n & 0xFF00) >> 1 | (n & 0xFF_0000) >> 2 | (n & 0xFF00_0000) >> 3
}

/// Decoder for an unsynchronized stream of bytes.
///
/// The decoder has an internal buffer.
pub struct Reader<R>
where
    R: io::Read,
{
    reader: R,
    buf: [u8; 8192],
    next: usize,
    available: usize,
    discard_next_null_byte: bool,
}

impl<R> Reader<R>
where
    R: io::Read,
{
    pub fn new(reader: R) -> Reader<R> {
        Reader {
            reader,
            buf: [0; 8192],
            next: 0,
            available: 0,
            discard_next_null_byte: false,
        }
    }
}

impl<R> io::Read for Reader<R>
where
    R: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut i = 0;

        while i < buf.len() {
            assert!(self.next <= self.available);
            if self.next == self.available {
                self.available = self.reader.read(&mut self.buf)?;
                self.next = 0;
                if self.available == 0 {
                    break;
                }
            }

            if self.discard_next_null_byte && self.buf[self.next] == 0x00 {
                self.discard_next_null_byte = false;
                self.next += 1;
                continue;
            }
            self.discard_next_null_byte = false;

            buf[i] = self.buf[self.next];
            i += 1;

            if self.buf[self.next] == 0xff {
                self.discard_next_null_byte = true;
            }
            self.next += 1;
        }

        Ok(i)
    }
}

/// Applies the unsynchronization scheme to a byte buffer.
pub fn encode_vec(buffer: &mut Vec<u8>) {
    let mut repeat_next_null_byte = false;
    let mut i = 0;
    while i < buffer.len() {
        if buffer[i] == 0x00 && repeat_next_null_byte {
            buffer.insert(i, 0);
            i += 1;
        }
        repeat_next_null_byte = buffer[i] == 0xFF;
        i += 1;
    }
}
