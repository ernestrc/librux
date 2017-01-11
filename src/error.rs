pub use nix::Error::Sys as SysError;
pub use nix::errno::*;

error_chain! {

    links { }

    foreign_links {
        IoError(::std::io::Error);
        ParseAddr(::std::net::AddrParseError);
        Utf8Error(::std::string::FromUtf8Error);
        NixError(::nix::Error);
    }

    errors {

        OutOfCapacity(max: usize) {
            description("out of capacity")
            display("OutOfCapacity: Buffer would exceed capacity of {} bytes", max)
        }
    }
}
