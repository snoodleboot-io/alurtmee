/// HTTP conditional-request validators cached for a single endpoint.
///
/// The poller replays these as `If-None-Match` / `If-Modified-Since` so GitHub can answer `304 Not
/// Modified` (free, unmetered) when nothing changed. Both fields are optional because a response
/// may carry an `ETag`, a `Last-Modified`, both, or neither. These are non-secret response headers
/// — no token ever lands here (ARD AD-6).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EtagRecord {
    /// The opaque `ETag` validator, replayed as `If-None-Match`.
    pub etag: Option<String>,
    /// The `Last-Modified` timestamp, replayed as `If-Modified-Since`.
    pub last_modified: Option<String>,
}
