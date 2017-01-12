use RawFd;
use epoll::EpollFd;
use handler::Handler;
use mux::{MuxCmd, MuxEvent};

pub trait HandlerFactory<'a, H, R>
  where H: Handler<MuxEvent<'a, R>, MuxCmd>,
        R: 'a,
{
  fn new_handler(&mut self, epfd: EpollFd, sockfd: RawFd) -> H;
  fn new_resource(&self) -> R;
}
