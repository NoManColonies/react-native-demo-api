use futures::future::{BoxFuture, FutureExt as _};
use hyper::Body;
use tonic::body::BoxBody;
use tower::Service;
use tracing::{field::Empty, info_span, Span};
use tracing_futures::Instrument;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TracingMiddleware<S> {
    pub inner: S,
}

impl<S> Service<hyper::Request<Body>> for TracingMiddleware<S>
where
    S: Service<hyper::Request<Body>, Response = hyper::Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
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

        let user_agent = req
            .headers()
            .get("User-Agent")
            .map(|h| h.to_str().unwrap_or(""))
            .unwrap_or("");
        let user_ip = req
            .headers()
            .get("X-Forwarded-For")
            .map(|h| h.to_str().unwrap_or(""))
            .unwrap_or("");
        let http_target = format!(
            "{}{}",
            req.uri().path(),
            req.uri()
                .query()
                .map(|query| format!("?{}", query))
                .unwrap_or_default()
        );
        let http_method = req.method();
        let http_version = req.version();
        let http_scheme = req
            .uri()
            .scheme()
            .map_or(Default::default(), |scheme| scheme.as_str());
        let request_id = Uuid::new_v4();

        let root_span = info_span!(
            "Incoming gRPC request",
            http.method = %http_method,
            http.route = &req.uri().path(),
            http.flavor = ?http_version,
            http.scheme = %http_scheme,
            http.target = %http_target,
            http.user_agent = %user_agent,
            http.user_ip = %user_ip,
            http.status = Empty,
            request_id = %request_id
        );

        async move {
            match inner.call(req).await {
                Ok(res) => {
                    Span::current().record("http.status", &&res.status().to_string()[..]);
                    Ok(res)
                }
                Err(e) => Err(e),
            }
        }
        .instrument(root_span)
        .boxed()
    }
}
