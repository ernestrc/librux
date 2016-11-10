use std::cmp;

use error::{Result, ErrorKind};

#[derive(Clone, Debug)]
pub struct ByteBuffer {
    next_write: usize,
    next_read: usize,
    capacity: usize,
    buf: Vec<u8>,
}

impl ByteBuffer {
    pub fn with_capacity(capacity: usize) -> ByteBuffer {
        ByteBuffer {
            next_read: 0,
            next_write: 0,
            capacity: capacity,
            buf: vec!(0_u8; capacity),
        }
    }

    #[inline]
    pub fn is_readable(&self) -> bool {
        self.readable() > 0
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        self.writable() > 0
    }

    #[inline]
    pub fn writable(&self) -> usize {
        self.capacity - self.next_write
    }

    #[inline]
    pub fn readable(&self) -> usize {
        self.next_write - self.next_read
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub fn write(&mut self, b: &[u8]) -> Result<usize> {
        let len = b.len();
        let wlen = self.next_write + len;
        if wlen > self.capacity {
            Err(ErrorKind::BufferOverflowError(self.capacity).into())
        } else {
            self.buf[self.next_write..wlen].copy_from_slice(b);
            self.extend(len);
            Ok(len)
        }
    }

    #[inline]
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let amt = cmp::min(self.next_write - self.next_read, buf.len());
        let (a, _) = self.buf[self.next_read..self.next_write].split_at(amt);
        buf[..amt].copy_from_slice(a);

        Ok(amt)
    }

    #[inline]
    pub fn slice<'a>(&'a self, offset: usize) -> &'a [u8] {
        &self.buf[self.next_read + offset..self.next_write]
    }

    #[inline]
    pub fn mut_slice<'a>(&'a mut self, offset: usize) -> &'a mut [u8] {
        &mut self.buf[self.next_read + offset..self.next_write]
    }

    #[inline]
    pub fn extend(&mut self, cnt: usize) {
        self.next_write += cnt;
    }

    #[inline]
    pub fn consume(&mut self, cnt: usize) {
        self.next_read += cnt;
        if self.next_read == self.next_write {
            self.clear();
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.next_read = 0;
        self.next_write = 0;
    }
}

impl<'a> From<&'a ByteBuffer> for &'a [u8] {
    fn from(b: &'a ByteBuffer) -> &'a [u8] {
        &b.buf[b.next_read..b.next_write]
    }
}

impl<'a> From<&'a mut ByteBuffer> for &'a mut [u8] {
    fn from(b: &'a mut ByteBuffer) -> &'a mut [u8] {
        &mut b.buf[b.next_write..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Cursor};
    use error::ErrorKind;

    #[test]
    fn does_buffer() {
        const size: usize = 32;
        let mut buffer = ByteBuffer::with_capacity(size);

        let a = [1, 2, 3];
        buffer.write(&a).unwrap();

        assert_eq!(buffer.readable(), 3);
        assert_eq!(buffer.writable(), size - 3);

        let mut b = [0; 3];
        let bcnt = buffer.read(&mut b).unwrap();
        assert_eq!(b, a);
        assert_eq!(bcnt, 3);

        let a2 = [4, 5, 6];
        buffer.write(&a2).unwrap();
        assert_eq!(buffer.readable(), 6);

        let mut c = [0; 3];
        let ccnt = buffer.read(&mut c).unwrap();
        assert_eq!(c, a);
        assert_eq!(ccnt, 3);

        buffer.consume(3);

        let mut e = [0; 3];
        let ecnt = buffer.read(&mut e).unwrap();
        assert_eq!(e, a2);
        assert_eq!(ecnt, 3);

        buffer.consume(3);

        let mut d = [0; 3];
        let dcnt = buffer.read(&mut d).unwrap();
        assert_eq!(dcnt, 0);
        assert!(d != a);
        assert_eq!(d, [0, 0, 0]);

        let z = [1, 2];
        buffer.write(&z).unwrap();

        let y = [3, 4];
        buffer.write(&y).unwrap();

        let mut w = [0];
        let wcnt = buffer.read(&mut w).unwrap();
        assert_eq!(wcnt, 1);
        assert_eq!(w, [1]);
        assert_eq!(buffer.readable(), 4);
        buffer.consume(1);
        assert_eq!(buffer.readable(), 3);
        assert_eq!(buffer.writable(), size - 4);

        let mut w2 = [0; 3];
        let wcnt2 = buffer.read(&mut w2).unwrap();
        assert_eq!(wcnt2, 3);
        assert_eq!(w2, [2, 3, 4]);
        assert_eq!(buffer.readable(), 3);
        buffer.consume(3);
        assert_eq!(buffer.readable(), 0);
        assert_eq!(buffer.writable(), size);

        let xz = [1; size];
        buffer.write(&xz).unwrap();
        assert_eq!(buffer.readable(), size);
        assert_eq!(buffer.writable(), 0);

        let mut w3 = [0; size];
        let wcnt3 = buffer.read(&mut w3).unwrap();
        assert_eq!(wcnt3, size);
        assert_eq!(w3, xz);
        assert_eq!(buffer.readable(), size);
        buffer.consume(size);
        assert_eq!(buffer.readable(), 0);
        assert_eq!(buffer.writable(), size);

        let xz = [1; size];
        buffer.write(&xz).unwrap();

        let mut w3 = [0; 10];
        let wcnt3 = buffer.read(&mut w3).unwrap();
        assert_eq!(wcnt3, 10);
        assert_eq!(w3, [1; 10]);
        assert_eq!(buffer.readable(), size);
        buffer.consume(wcnt3);
        assert_eq!(buffer.readable(), size - 10);
        assert_eq!(buffer.writable(), 0);

        let mut w4 = [0; 22];
        let wcnt4 = buffer.read(&mut w4).unwrap();
        assert_eq!(wcnt4, 22);
        assert_eq!(w4, [1; 22]);
        assert_eq!(buffer.readable(), size - 10);
        buffer.consume(wcnt4);
        assert_eq!(buffer.readable(), 0);
        assert_eq!(buffer.writable(), size);

        buffer.write(&[1; size]).unwrap();

        assert!(buffer.write(&[1; 1]).is_err());

        buffer.clear();
        assert_eq!(buffer.readable(), 0);
        assert_eq!(buffer.writable(), size);

    }

    #[test]
    fn share_read_ref() {
        let mut buffer = ByteBuffer::with_capacity(32);

        let a = [4, 5, 6];
        buffer.write(&a).unwrap();

        assert_eq!(buffer.readable(), 3);

        let mut b: Vec<u8> = vec![1; 3];

        b.extend_from_slice(From::from(&buffer));

        assert_eq!(b, vec![1, 1, 1, 4, 5, 6]);
    }

    #[test]
    fn share_write_ref() {
        let mut buffer = ByteBuffer::with_capacity(32);

        let a = [4, 5];
        buffer.write(&a).unwrap();

        let mut b = Cursor::new([1, 2, 3]);

        let r = {
            let dst: &mut [u8] = From::from(&mut buffer);
            b.read(dst).unwrap()
        };

        assert_eq!(r, 3);
        buffer.extend(r);

        assert_eq!(buffer.readable(), 5);

        let mut c = [0; 5];
        let ccnt = buffer.read(&mut c).unwrap();
        assert_eq!(c, [4, 5, 1, 2, 3]);
        assert_eq!(ccnt, 5);
    }

    #[test]
    fn overflow_error() {
        let mut buffer = ByteBuffer {
            next_read: 0,
            capacity: 5,
            next_write: 0,
            buf: Vec::with_capacity(5),
        };

        let a = [1, 2, 3, 4, 5, 6];
        let res = buffer.write(&a);

        assert!(res.is_err());
        match res.err().unwrap().into_kind() {
            ErrorKind::BufferOverflowError(max) => assert_eq!(max, 5),
            e => panic!("different error: {:?}", e),
        }
    }
}
