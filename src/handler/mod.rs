pub mod mux;

// TODO model like std Iterator
pub trait Handler<E> {
    fn is_terminated(&self) -> bool;
    fn ready(&mut self, e: &E);
}
