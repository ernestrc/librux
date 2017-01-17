pub use nix::Error as NixError;
pub use nix::errno;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    links { }

    foreign_links {
        IoError(::std::io::Error);
        ParseAddr(::std::net::AddrParseError);
        Utf8Error(::std::string::FromUtf8Error);
        NixError(NixError);
    }

    errors {

        OutOfCapacity(max: usize) {
            description("out of capacity")
            display("OutOfCapacity: Buffer would exceed capacity of {} bytes", max)
        }
    }
}
