use super::service::SentrySessionTracker;
use sentry_core::Hub;
use std::sync::Arc;
use tower::Layer;

/// A helper construct that can be used to reconfigure and build the middleware.
pub struct SentrySessionLayerBuilder {
    middleware: SentrySessionLayer,
}

impl SentrySessionLayerBuilder {
    /// Finishes the building and returns a middleware
    pub fn finish(self) -> SentrySessionLayer {
        self.middleware
    }

    #[allow(dead_code)]
    /// Reconfigures the middleware so that it uses a specific hub instead of the default one.
    pub fn with_hub(mut self, hub: Arc<Hub>) -> Self {
        self.middleware.hub = Some(hub);
        self
    }

    #[allow(dead_code)]
    /// Reconfigures the middleware so that it uses a specific hub instead of the default one.
    pub fn with_default_hub(mut self) -> Self {
        self.middleware.hub = None;
        self
    }

    /// If configured the sentry id is attached to a X-Sentry-Event header.
    pub fn emit_header(mut self, val: bool) -> Self {
        self.middleware.emit_header = val;
        self
    }

    #[allow(dead_code)]
    /// Enables or disables error reporting.
    ///
    /// The default is to report all errors.
    pub fn capture_server_errors(mut self, val: bool) -> Self {
        self.middleware.capture_server_errors = val;
        self
    }
}

#[derive(Debug, Clone)]
pub struct SentrySessionLayer {
    hub: Option<Arc<Hub>>,
    emit_header: bool,
    capture_server_errors: bool,
}

impl SentrySessionLayer {
    /// Creates a new sentry middleware.
    pub fn new() -> Self {
        SentrySessionLayer {
            hub: None,
            emit_header: false,
            capture_server_errors: true,
        }
    }

    /// Creates a new middleware builder.
    pub fn builder() -> SentrySessionLayerBuilder {
        SentrySessionLayer::new().into_builder()
    }

    /// Converts the middleware into a builder.
    pub fn into_builder(self) -> SentrySessionLayerBuilder {
        SentrySessionLayerBuilder { middleware: self }
    }

    pub fn get_hub(&self) -> &Option<Arc<Hub>> {
        &self.hub
    }

    pub fn get_capture_server_errors(&self) -> bool {
        self.capture_server_errors
    }

    #[allow(dead_code)]
    pub fn get_emit_header(&self) -> bool {
        self.emit_header
    }
}

impl Default for SentrySessionLayer {
    fn default() -> Self {
        SentrySessionLayer::new()
    }
}

impl<S> Layer<S> for SentrySessionLayer {
    type Service = SentrySessionTracker<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SentrySessionTracker {
            inner,
            session: self.clone(),
        }
    }
}
