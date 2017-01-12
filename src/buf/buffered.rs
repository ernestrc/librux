use buf::*;
use error::*;
use std::cmp;

pub trait Buffered
  where Self: Sized,
{
  fn max_size() -> usize;

  fn from_buffer(buffer: &mut ByteBuffer) -> Result<Option<Self>>;

  fn to_buffer(self, buffer: &mut ByteBuffer) -> Result<Option<Self>>;
}

pub fn buffer<T: Buffered>(msg: T, buf: &mut ByteBuffer) -> Result<()> {
  match msg.to_buffer(buf) {
    Ok(Some(msg)) => {
      let cap = buf.capacity();
      let max_size = T::max_size();

      if cap == max_size {
        bail!(ErrorKind::OutOfCapacity(max_size));
      }

      let reserve_exact = cmp::min(cap + cap, max_size);

      if reserve_exact == max_size {
        buf.reserve(reserve_exact - cap);
        msg.to_buffer(buf)?;
        return Ok(());
      }

      buf.reserve(reserve_exact - cap);

      buffer(msg, buf)
    }
    Ok(None) => Ok(()),
    Err(e) => Err(e),
  }
}
