use RawFd;
use handler::Handler;
use poll::EpollFd;

pub trait HandlerFactory<'p, H, In, Out>
  where H: Handler<'p, In, Out>,
{
  fn done(&mut self, handler: H, index: usize);
  fn new(&'p mut self, epfd: EpollFd, index: usize, reason: RawFd) -> H;
}
