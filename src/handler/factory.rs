use RawFd;
use handler::Handler;
use poll::EpollFd;

pub trait HandlerFactory<'res, H, In, Out>
  where H: Handler<In, Out>, // TODO + Drop
{

  type Resource: Default + Clone + 'res;

  fn new(&mut self, epfd: EpollFd, sockfd: RawFd, resource: &'res mut Self::Resource) -> H;
}
