use poll::EpollEvent;

pub mod echo;
pub mod sync;

// Epoll Events handler.
pub trait Handler {
    fn ready(&mut self, events: &EpollEvent);
}
