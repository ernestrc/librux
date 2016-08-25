use poll::EpollEvent;

pub mod echo;
pub mod sync;

// Epoll Events handler.
// TODO abstract over what is ready for?
pub trait Handler<E> {
    fn is_terminated(&self) -> bool;
    fn ready(&mut self, e: &E);
}

// pub trait Controller where Self: Handler<EpollEvent> {
//     fn hup(&mut self, fd: RawFd);
// }
