pub trait Handler<In, Out> {
  #[inline]
  fn on_next(&mut self, In) -> Out;
}
