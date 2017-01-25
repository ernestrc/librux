use Reset;
use error::*;
use std::io::{Read, Cursor};
use super::*;

#[test]
fn does_buffer() {
  const SIZE: usize = 32;
  let mut buffer = ByteBuffer::with_capacity(SIZE);

  let a = [1, 2, 3];
  buffer.write(&a).unwrap();

  assert_eq!(buffer.readable(), 3);
  assert_eq!(buffer.writable(), SIZE - 3);

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
  assert_eq!(buffer.writable(), SIZE - 4);

  let mut w2 = [0; 3];
  let wcnt2 = buffer.read(&mut w2).unwrap();
  assert_eq!(wcnt2, 3);
  assert_eq!(w2, [2, 3, 4]);
  assert_eq!(buffer.readable(), 3);
  buffer.consume(3);
  assert_eq!(buffer.readable(), 0);
  assert_eq!(buffer.writable(), SIZE);

  let xz = [1; SIZE];
  buffer.write(&xz).unwrap();
  assert_eq!(buffer.readable(), SIZE);
  assert_eq!(buffer.writable(), 0);

  let mut w3 = [0; SIZE];
  let wcnt3 = buffer.read(&mut w3).unwrap();
  assert_eq!(wcnt3, SIZE);
  assert_eq!(w3, xz);
  assert_eq!(buffer.readable(), SIZE);
  buffer.consume(SIZE);
  assert_eq!(buffer.readable(), 0);
  assert_eq!(buffer.writable(), SIZE);

  let xz = [1; SIZE];
  buffer.write(&xz).unwrap();

  let mut w3 = [0; 10];
  let wcnt3 = buffer.read(&mut w3).unwrap();
  assert_eq!(wcnt3, 10);
  assert_eq!(w3, [1; 10]);
  assert_eq!(buffer.readable(), SIZE);
  buffer.consume(wcnt3);
  assert_eq!(buffer.readable(), SIZE - 10);
  assert_eq!(buffer.writable(), 0);

  let mut w4 = [0; 22];
  let wcnt4 = buffer.read(&mut w4).unwrap();
  assert_eq!(wcnt4, 22);
  assert_eq!(w4, [1; 22]);
  assert_eq!(buffer.readable(), SIZE - 10);
  buffer.consume(wcnt4);
  assert_eq!(buffer.readable(), 0);
  assert_eq!(buffer.writable(), SIZE);

  buffer.write(&[1; SIZE]).unwrap();

  assert!(buffer.write(&[1; 1]).is_err());

  buffer.reset();
  assert_eq!(buffer.readable(), 0);
  assert_eq!(buffer.writable(), SIZE);

}

#[test]
fn write_at_index() {
  const SIZE: usize = 32;
  let mut buffer = ByteBuffer::with_capacity(SIZE);

  let a = [1, 2, 3];
  buffer.write(&a).unwrap();

  assert_eq!(buffer.readable(), 3);
  assert_eq!(buffer.writable(), SIZE - 3);

  buffer.write_at(1, &[2, 2]).unwrap();
  assert_eq!(buffer.readable(), 5);
  assert_eq!(buffer.writable(), SIZE - 5);

  let mut b = [0; 5];
  let bcnt = buffer.read(&mut b).unwrap();
  assert_eq!(b, [1, 2, 2, 2, 3]);
  assert_eq!(bcnt, 5);
}

#[test]
fn share_read_ref() {
  let mut buffer = ByteBuffer::with_capacity(32);

  let a = [4, 5, 6];
  buffer.write(&a).unwrap();

  assert_eq!(buffer.readable(), 3);

  let mut b: Vec<u8> = vec![1; 3];

  b.extend_from_slice(From::from(&buffer));

  assert_eq!(b, [1, 1, 1, 4, 5, 6]);
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
fn reserves_additional_capacity() {
  let mut buffer = ByteBuffer::with_capacity(5);

  let a = [1, 2, 3, 4, 5];
  buffer.write(&a).unwrap();
  let mut res = buffer.write(&[1]);

  assert!(res.is_err());
  match res.err().unwrap().kind() {
    &ErrorKind::OutOfCapacity(max) => assert_eq!(max, 5),
    e => panic!("different error: {:?}", e),
  }

  buffer.reserve(1);
  res = buffer.write(&[6]);

  assert!(res.is_ok());

  assert_eq!(res.unwrap(), 1);
  assert_eq!(buffer.readable(), 6);
  assert_eq!(buffer.writable(), 0);

  let mut b = [0; 6];
  buffer.read(&mut b).unwrap();

  assert_eq!(b, [1, 2, 3, 4, 5, 6]);
}

#[test]
fn returns_out_of_capacity_error() {
  let mut buffer = ByteBuffer::with_capacity(5);

  let a = [1, 2, 3, 4, 5];
  buffer.write(&a).unwrap();
  let res = buffer.write(&[1]);

  assert!(res.is_err());
  match res.err().unwrap().kind() {
    &ErrorKind::OutOfCapacity(max) => assert_eq!(max, 5),
    e => panic!("different error: {:?}", e),
  }
}

#[test]
fn compacts_buffer_when_out_of_capacity() {
  let mut buffer = ByteBuffer::with_capacity(5);

  let a = [1, 2, 3, 4, 5];
  buffer.write(&a).unwrap();

  let mut aa = [0; 2];
  assert_eq!(buffer.read(&mut aa).unwrap(), 2);
  assert!(aa == [1, 2]);

  buffer.consume(2);

  let mut res = buffer.write(&[1, 2]);
  assert!(res.is_ok());

  let mut bb = [0; 5];
  assert_eq!(buffer.read(&mut bb).unwrap(), 5);
  assert!(bb == [3, 4, 5, 1, 2]);

  res = buffer.write(&[1, 2]);

  match res.err().unwrap().kind() {
    &ErrorKind::OutOfCapacity(max) => assert_eq!(max, 5),
    e => panic!("different error: {:?}", e),
  }
}

struct MyType {
  vec: Vec<u8>,
}

impl Buffered for MyType {
  type Error = Error;
  fn max_size() -> usize {
    30
  }

  fn to_buffer(self, buffer: &mut ByteBuffer) -> Result<Option<MyType>> {
    let len = self.vec.len();
    if len > buffer.writable() {
      return Ok(Some(self));
    }
    buffer.write(&self.vec)?;
    Ok(None)
  }

  fn from_buffer(buffer: &mut ByteBuffer) -> Result<Option<MyType>> {
    unimplemented!()
  }
}

#[test]
fn buffer_method_buffers() {
  let t = MyType { vec: vec!(2; 10) };

  let mut buf = ByteBuffer::with_capacity(20);
  buffer(t, &mut buf).unwrap();

  assert_eq!(buf.readable(), 10);
  assert_eq!(buf.writable(), 10);

  let mut r = [0; 10];
  buf.read(&mut r).unwrap();

  assert_eq!(r, [2; 10]);
}

#[test]
fn buffer_method_extends_capacity() {
  {
    let mut t = MyType { vec: vec!(2; 30) };

    let mut buf = ByteBuffer::with_capacity(20);
    buffer(t, &mut buf).unwrap();

    assert_eq!(buf.readable(), 30);
    assert_eq!(buf.writable(), 0);

    let mut r = [0; 30];
    buf.read(&mut r).unwrap();

    assert_eq!(r, [2; 30]);

    t = MyType { vec: vec!(0; 1) };
    let res = buffer(t, &mut buf);
    assert!(res.is_err());

    assert!(format!("{}", res.unwrap_err()).contains("30"));
  }
  {
    let mut t = MyType { vec: vec!(2; 20) };

    let mut buf = ByteBuffer::with_capacity(10);
    buffer(t, &mut buf).unwrap();

    assert_eq!(buf.readable(), 20);
    assert_eq!(buf.writable(), 0);

    let mut r = [0; 20];
    buf.read(&mut r).unwrap();

    assert_eq!(r, [2; 20]);
    assert_eq!(buf.readable(), 20);
    assert_eq!(buf.writable(), 0);

    {
      t = MyType { vec: vec!(0; 1) };
      let res = buffer(t, &mut buf);
      res.unwrap();
    }

    {
      t = MyType { vec: vec!(0; 10) };
      let res = buffer(t, &mut buf);
      assert!(res.is_err());
      assert!(format!("{}", res.unwrap_err()).contains("30"));
    }

    // resets capacity to initial capacity
    buf.reset();

    let a = [1; 21];
    let res = buf.write(&a);

    assert!(res.is_err());
    match res.err().unwrap().kind() {
      &ErrorKind::OutOfCapacity(max) => assert_eq!(max, 10),
      e => panic!("different error: {:?}", e),
    }
  }
}
