use poll::EpollEvent;
use error::Result;

pub mod echo;
pub mod sync;

pub trait Handler {
    fn ready(&mut self, events: &EpollEvent);
}
