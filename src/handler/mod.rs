mod factory;

use poll::EpollFd;
pub use self::factory::HandlerFactory;

pub trait Handler<'h, In, Out> {
  #[inline]
  fn on_next(&'h mut self, In) -> Out;
}

impl<'h, T, In, Out> Handler<'h, In, Out> for T
  where T: FnMut(In) -> Out,
{
  fn on_next(&'h mut self, event: In) -> Out {
    self(event)
  }
}

pub trait Reset {
  #[inline]
  fn reset(&mut self, epfd: EpollFd);
}
