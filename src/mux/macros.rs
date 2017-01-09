#[macro_export]
macro_rules! keep_or {
  ($cmd:expr, $b: block) => {{
    match $cmd {
      MuxCmd::Close => $b,
      _ => (),
    }
  }}
}

#[macro_export]
macro_rules! keep {
  ($cmd:expr) => {{
    keep_or_return!($cmd, MuxCmd::Close)
  }}
}
