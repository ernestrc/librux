mod factory;

use poll::EpollFd;
pub use self::factory::HandlerFactory;

pub trait Handler<In, Out> {
  #[inline]
  fn on_next(&mut self, In) -> Out;

  // TODO?
  // #[inline]
  // fn on_err(&mut self, Err);
}

// impl<'h, T, In, Out> Handler<'h, In, Out> for T
//   where T: FnMut(In) -> Out,
// {
//   fn on_next(&'h mut self, event: In) -> Out {
//     self(event)
//   }
// }

pub trait Reset {
  #[inline]
  fn reset(&mut self, epfd: EpollFd);
}
