pub trait Handler<In, Out> {

  fn on_next(&mut self, In);

  fn next(&mut self) -> Out;
}
