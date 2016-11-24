pub mod mux;

pub trait Handler {
    type In;
    type Out;
    fn ready(&mut self, Self::In) -> Self::Out;
}
