pub mod sync;

pub trait Handler<E> {
    fn is_terminated(&self) -> bool;
    fn ready(&mut self, e: &E);
}
