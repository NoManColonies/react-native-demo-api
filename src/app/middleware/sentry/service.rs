use super::layer::SentrySessionLayer;
use crate::app::util::error::ServiceError;
use futures::future::{BoxFuture, FutureExt as _};
use hyper::Body;
use sentry_core::{
    protocol::{ClientSdkPackage, Event, Request},
    Breadcrumb, Hub, Level, SentryFutureExt,
};
use std::{borrow::Cow, boxed::Box, sync::Arc};
use tonic::{body::BoxBody, transport::Error};
use tower::{BoxError, Service};
use tracing::error;

#[derive(Debug, Clone)]
pub struct SentrySessionTracker<S> {
    pub inner: S,
    pub session: SentrySessionLayer,
}

impl<S> Service<hyper::Request<Body>> for SentrySessionTracker<S>
where
    S: Service<hyper::Request<Body>, Response = hyper::Response<BoxBody>, Error = BoxError>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: hyper::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let session = self.session.clone();
        let hub = Arc::new(Hub::new_from_top(
            session.get_hub().clone().unwrap_or_else(Hub::main),
        ));
        let client = hub.client();
        let track_sessions = client.as_ref().map_or(false, |client| {
            let options = client.options();
            options.auto_session_tracking
                && options.session_mode == sentry_core::SessionMode::Request
        });
        if track_sessions {
            hub.start_session();
        }
        let with_pii = client
            .as_ref()
            .map_or(false, |client| client.options().send_default_pii);

        let (tx, sentry_req) = sentry_request_from_http(&req, with_pii);
        hub.configure_scope(|scope| {
            scope.set_transaction(tx.as_deref());
            scope.add_event_processor(Box::new(move |event| {
                Some(process_event(event, &sentry_req))
            }))
        });

        async move {
            match inner.call(req).bind_hub(hub.clone()).await {
                Ok(res) => Ok(res),
                Err(err) => {
                    if session.get_capture_server_errors() {
                        capture_boxed_error(&err, hub);
                    }
                    Err(err)
                }
            }
        }
        .boxed()
    }
}

fn capture_boxed_error(err: &BoxError, hub: Arc<Hub>) {
    if let Some(e) = err.downcast_ref::<Error>() {
        // downcast to `tonic::transport::Error`
        error!("failure in service layer: {:?}", e);
        hub.capture_error(e);
    } else if let Some(e) = err.downcast_ref::<ServiceError>() {
        // downcast to `crate::app::util::error::GeekyRepercussion`
        error!("failure in service layer: {:?}", e);
        hub.add_breadcrumb(Breadcrumb {
            ty: "service layer".to_string(),
            message: Some(format!("failure in service layer: {:?}", e)),
            ..Default::default()
        });
        hub.capture_message(
            "Service encountered failure while attempting to process service layer",
            Level::Error,
        );
    }
}

/// Build a Sentry request struct from the HTTP request
fn sentry_request_from_http(
    request: &hyper::Request<Body>,
    with_pii: bool,
) -> (Option<String>, Request) {
    let transaction = Some(format!(
        "{}{}",
        request.uri().path(),
        request
            .uri()
            .query()
            .map(|query| format!("?{}", query))
            .unwrap_or_default()
    ));

    let mut sentry_req = Request {
        url: request.uri().to_string().parse().ok(),
        method: Some(request.method().to_string()),
        headers: request
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect(),
        ..Default::default()
    };

    // If PII is enabled, include the remote address
    if with_pii {
        if let Some(Ok(remote)) = request
            .headers()
            .get("X-Forwarded-For")
            .map(|header_val| header_val.to_str().map(|val| val.to_string()))
        {
            sentry_req.env.insert("REMOTE_ADDR".into(), remote);
        }
    };

    (transaction, sentry_req)
}

/// Add request data to a Sentry event
fn process_event(mut event: Event<'static>, request: &Request) -> Event<'static> {
    // Request
    if event.request.is_none() {
        event.request = Some(request.clone());
    }

    // SDK
    if let Some(sdk) = event.sdk.take() {
        let mut sdk = sdk.into_owned();
        sdk.packages.push(ClientSdkPackage {
            name: "sentry-tonic".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        });
        event.sdk = Some(Cow::Owned(sdk));
    }
    event
}
