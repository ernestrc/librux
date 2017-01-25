#[macro_export]
macro_rules! report_err {
  ($err:expr) => {{
    let e: Error = $err;
    error!("{}\n{:?}", e, e.backtrace());

    for e in e.iter().skip(1) {
      error!("caused_by: {}", e);
    }
  }}
}

#[macro_export]
macro_rules! syscall {
  ($syscall:expr) => {{
    use $crate::error::*;

    let res: Result<_>;
    loop {
      match $syscall {
        Ok(m) => {
          res = Ok(Some(m));
          break;
        },
        Err(NixError::Sys(errno::EAGAIN)) => {
          trace!("EAGAIN: not ready");
          res = Ok(None);
          break;
        },
        Err(NixError::Sys(errno::EINTR)) => {
          debug!("EINTR: re-submitting syscall");
          continue;
        },
        Err(err) => {
          res = Err(err.into());
          break;
        }
      }
    }
    res
  }}
}
