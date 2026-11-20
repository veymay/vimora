//! Building blocks of Vimora's screen sharing, each testable alone (see `docs/SCREENSHARE.md`):
//!
//! * [`probe`] — what this computer can really do (session, GPU, encoders — by running them);
//! * [`queue`] — the bounded "newest frame wins" queue between capture and encoder;
//! * [`backoff`] — bounded exponential retry delays;
//! * [`machine`] — the explicit session state machine (timeouts, failures, retries);
//! * [`profile`] — data-driven receiver profiles;
//! * [`wfd`] — Wi-Fi Display (Miracast) information elements.

pub mod backoff;
pub mod machine;
pub mod probe;
pub mod profile;
pub mod queue;
pub mod wfd;
