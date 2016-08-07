
error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links { }

    foreign_links {
        ::std::io::Error, IoError;
        ::std::net::AddrParseError, ParseAddr;
        ::std::string::FromUtf8Error, Utf8Error;
        ::nix::Error, NixError;
    }

    errors {

        BufferOverflowError(max: usize) {
            description("Buffer overflow error")
                display("Buffer exceeded max of {} bytes", max)
        }
    }
}
