#[macro_export]
macro_rules! report_err {
  ($name:expr, $err:expr) => {{
    let e: Error = $err;
    error!("{}: {}\n{:?}", $name, e, e.backtrace());

    for e in e.iter().skip(1) {
      error!("caused_by: {}", e);
    }
  }}
}

#[macro_export]
macro_rules! eintr {
  ($syscall:expr, $name:expr, $($arg:expr),*) => {{
    let res;
    loop {
      match $syscall($($arg),*) {
        Ok(m) => {
          res = Ok(Some(m));
          break;
        },
        Err(SysError(EAGAIN)) => {
          trace!("{}: EAGAIN: not ready", $name);
          res = Ok(None);
          break;
        },
        Err(SysError(EINTR)) => {
          debug!("{}: EINTR: re-submitting syscall", $name);
          continue;
        },
        Err(err) => {
          report_err!($name, err.into());
          res = Err(err);
          break;
        }
      }
    }
    res
  }}
}

#[macro_export]
macro_rules! perror {
  ($name:expr, $res:expr) => {{
    match $res {
      Ok(s) => s,
      Err(e) => {
        report_err!($name, e.into());
      },
    }
  }}
}
