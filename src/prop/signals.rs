use handler::Handler;
pub use nix::sys::signal::{Signal, SigSet};

use poll::EpollCmd;

pub struct DefaultSigHandler;

impl Handler for DefaultSigHandler {
    type In = Signal;
    type Out = EpollCmd;

    fn ready(&mut self, _: Signal) -> EpollCmd {
        EpollCmd::Shutdown
    }
}
