use poll::EpollEvent;

pub mod echo;
pub mod sync;

// Epoll Events handler.
// TODO abstract over what is ready for?
pub trait Handler {
    fn is_terminated(&self) -> bool;
    fn ready(&mut self, events: &EpollEvent);
}
