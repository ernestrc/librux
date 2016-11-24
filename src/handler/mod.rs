pub mod mux;

pub trait Handler {
    type In;
    type Out;
    fn reset(&mut self);
    fn ready(&mut self, Self::In) -> Option<Self::Out>;
}
