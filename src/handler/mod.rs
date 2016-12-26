mod factory;

use poll::EpollFd;
pub use self::factory::HandlerFactory;

pub trait Handler<In, Out> {
  #[inline]
  fn on_next(&mut self, In) -> Out;
}

impl<T, In, Out> Handler<In, Out> for T
  where T: FnMut(In) -> Out,
{
  fn on_next(&mut self, event: In) -> Out {
    self(event)
  }
}

pub trait Reset {
  #[inline]
  fn reset(&mut self, epfd: EpollFd);
}
