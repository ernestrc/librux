use handler::Handler;
use poll::EpollFd;
use RawFd;

pub trait HandlerFactory<'p, In, Out> {
  type H: Handler<In, Out>;

  fn done(&mut self, handler: Self::H, index: usize);
  fn new(&'p mut self, epfd: EpollFd, index: usize, reason: RawFd) -> Self::H;
}
