/// The outcome of a conditional `GET .../pulls` request.
///
/// GitHub's conditional requests (`If-None-Match` + `ETag`) let the poller ask "has anything
/// changed since the ETag I last saw?". A `304 Not Modified` answer is *free* (it does not consume
/// rate-limit budget, AD-1) and carries no body, so we model the two cases distinctly rather than
/// returning an empty `Vec` that would be ambiguous with "no open PRs".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrOutcome {
    /// GitHub returned `304 Not Modified`: the cached data is still current; no body was read.
    NotModified,
    /// GitHub returned `200` with a fresh list of open pull requests.
    Modified(Vec<domain::PullRequest>),
}
