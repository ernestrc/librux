use RawFd;

pub enum Action {
  Notify(usize, RawFd),
  New(u64),
}

impl Action {
  #[inline(always)]
  pub fn encode(action: Action) -> u64 {
    match action {
      Action::Notify(data, fd) => ((fd as u64) << 31) | ((data as u64) << 15) | 0,
      Action::New(data) => data,
    }
  }

  #[inline(always)]
  pub fn decode(data: u64) -> Action {
    let arg1 = ((data >> 15) & 0xffff) as usize;
    let fd = (data >> 31) as i32;
    match data & 0x7fff {
      0 => Action::Notify(arg1, fd),
      _ => Action::New(data),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::Action;
  #[test]
  fn decode_encode_new_action() {
    let data = Action::encode(Action::New(::std::u64::MAX));

    if let Action::New(fd) = Action::decode(data) {
      assert!(fd == ::std::u64::MAX);
    } else {
      panic!("action is not Action::New")
    }
  }

  #[test]
  fn decode_encode_notify_action() {
    let data = Action::encode(Action::Notify(10110, 0));

    if let Action::Notify(data, fd) = Action::decode(data) {
      assert!(data == 10110);
      assert!(fd == 0);
    } else {
      panic!("action is not Action::Notify")
    }
  }
}
