#[macro_export]
macro_rules! keep_or {
  ($cmd:expr, $b: block) => {{
    if let MuxCmd::Close = $cmd {
      $b
    }
  }}
}

#[macro_export]
macro_rules! keep {
  ($cmd:expr) => {{
    keep_or!($cmd, { return MuxCmd::Close; });
  }}
}
