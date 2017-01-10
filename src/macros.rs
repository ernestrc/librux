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
macro_rules! eintr {
  ($syscall:expr) => {{
    let res;
    loop {
      match $syscall {
        Ok(m) => {
          res = Ok(Some(m));
          break;
        },
        Err(SysError(EAGAIN)) => {
          trace!("EAGAIN: not ready");
          res = Ok(None);
          break;
        },
        Err(SysError(EINTR)) => {
          debug!("EINTR: re-submitting syscall");
          continue;
        },
        Err(err) => {
          report_err!(err.into());
          res = Err(err);
          break;
        }
      }
    }
    res
  }}
}
