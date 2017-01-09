use RawFd;
use handler::Handler;
use mux::{MuxCmd, MuxEvent};
use poll::EpollFd;

pub trait HandlerFactory<'a, H, R>
  where H: Handler<MuxEvent<'a, R>, MuxCmd>,
        R: 'a,
{
  fn new_handler(&mut self, epfd: /* TODO maybe EpollEvent should be generic over data + Into<u64> */EpollFd, sockfd: RawFd) -> H;
  fn new_resource(&self) -> R;
}
