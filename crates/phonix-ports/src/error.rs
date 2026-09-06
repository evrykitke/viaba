//! What a port may fail with.

/// What can go wrong reaching another app.
///
/// Deliberately short. A caller across an app boundary can only carry on
/// without the answer, refuse its own operation, or give up — anything more
/// specific belongs inside the provider. `ServiceError` is not used here: it
/// lives above every app, and its `Forbidden` would surface in the consumer as
/// if the consumer's own caller had been refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    /// The provider is present and could not answer. Not an answer, so the
    /// caller's own operation should fail with it — treating it as "no cost
    /// centres exist" would post to the wrong place.
    #[error("the {port} port failed: {detail}")]
    Unavailable { port: &'static str, detail: String },

    /// The provider answered, and the answer is no. Rendered beside the control
    /// that caused it rather than treated as a fault.
    #[error("{0}")]
    Refused(phonix_core::Message),
}

impl PortError {
    pub fn unavailable(port: &'static str, detail: impl std::fmt::Display) -> Self {
        Self::Unavailable {
            port,
            detail: detail.to_string(),
        }
    }
}
