
/// defining errors while parsing bytes to http headers
#[derive(Debug)]
 pub enum CreatingHeadersErrors {
    /// for invalid headers format
    InvalidFormat,
    /// max headers size
    MaxHeadersSizeReachedOut,
    /// if headers payload not enough
    ReadMore,
    /// when incoming header contains malicious attack or payload
    DangerousInvalidFormat
 }

impl<T> Into<Result<T,CreatingHeadersErrors>> for  CreatingHeadersErrors {
    fn into(self) -> Result<T, CreatingHeadersErrors> {
        Err(self)
    }
}