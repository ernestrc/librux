use Reset;
use error::{Result, ErrorKind};
use std::cmp;
use std::io;
use std::ptr;

static DEFAULT_BUF_SIZE: &'static usize = &(1024 * 16);

// TODO define trait and implement UserBuffer and KernelBuffer
// TODO specialized `copy_from`
#[derive(Debug, Clone)]
pub struct ByteBuffer {
  init_capacity: usize,
  next_write: usize,
  next_read: usize,
  capacity: usize,
  buf: Vec<u8>,
}

pub struct Mark {
  pub next_write: usize,
  pub next_read: usize,
}

impl ByteBuffer {
  pub fn with_capacity(capacity: usize) -> ByteBuffer {
    ByteBuffer {
      init_capacity: capacity,
      next_read: 0,
      next_write: 0,
      capacity: capacity,
      buf: vec!(0_u8; capacity),
    }
  }

  #[inline]
  pub fn mark(&self) -> Mark {
    Mark {
      next_write: self.next_write,
      next_read: self.next_read,
    }
  }

  #[inline]
  pub fn reset_from(&mut self, mark: Mark) {
    self.next_write = mark.next_write;
    self.next_read = mark.next_read;
  }

  #[inline]
  pub fn reserve(&mut self, additional: usize) {
    self.buf.append(&mut vec!(0_u8; additional));
    self.capacity += additional;
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

  pub fn compact(&mut self) -> bool {
    self.next_read > 0 &&
    {
      let len = self.next_write - self.next_read;
      unsafe {
        let p = self.buf.as_mut_ptr();
        ptr::copy(p.offset(self.next_read as isize), p, len);
      }
      self.next_write = len;
      self.next_read = 0;
      true
    }
  }

  #[inline]
  pub fn write(&mut self, b: &[u8]) -> Result<usize> {
    let len = b.len();
    let wlen = self.next_write + len;
    if wlen > self.capacity {
      if !self.compact() {
        bail!(ErrorKind::OutOfCapacity(self.capacity))
      }
      self.write(b)
    } else {
      self.buf[self.next_write..wlen].copy_from_slice(b);
      self.extend(len);
      Ok(len)
    }
  }

  #[inline]
  pub fn write_at(&mut self, index: usize, b: &[u8]) -> Result<usize> {
    assert!(index >= self.next_read && index < self.next_write);

    let len = b.len();
    let wlen = self.next_write + len;
    if wlen > self.capacity {
      if !self.compact() {
        bail!(ErrorKind::OutOfCapacity(self.capacity))
      }
      self.write_at(index, b)
    } else {
      unsafe {
        {
          let p = self.buf.as_mut_ptr();
          ptr::copy(p.offset(index as isize), p.offset((index + len) as isize), wlen);
        }
      }

      self.buf[index..index + len].copy_from_slice(b);
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
    &mut self.buf[self.next_write + offset..]
  }

  #[inline]
  pub fn extend(&mut self, cnt: usize) {
    self.next_write += cnt;
  }

  #[inline]
  pub fn consume(&mut self, cnt: usize) {
    self.next_read += cnt;
    if self.next_read == self.next_write {
      self.reset();
    }
  }
}

impl Default for ByteBuffer {
  fn default() -> ByteBuffer {
    ByteBuffer::with_capacity(*DEFAULT_BUF_SIZE)
  }
}

impl Reset for ByteBuffer {
  #[inline]
  fn reset(&mut self) {
    self.buf.truncate(self.init_capacity);
    self.capacity = self.init_capacity;
    self.next_read = 0;
    self.next_write = 0;
  }
}

impl io::Write for ByteBuffer {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    match self.write(buf) {
      Ok(u) => Ok(u),
      Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("io::Write: {}", e))),
    }
  }
  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl io::Read for ByteBuffer {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    match self.read(buf) {
      Ok(u) => Ok(u),
      Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("io::Read: {}", e))),
    }
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

impl<'a> From<&'a mut ByteBuffer> for &'a [u8] {
  fn from(b: &'a mut ByteBuffer) -> &'a [u8] {
    &b.buf[b.next_write..]
  }
}
