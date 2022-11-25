use futures::future::{BoxFuture, FutureExt as _};
use hyper::Body;
use redis::aio::ConnectionManager;
use tonic::body::BoxBody;
use tower::Service;

#[derive(Clone)]
pub struct ConfigMiddleware<S> {
    pub inner: S,
    pub redis_pool: ConnectionManager,
}

impl<S> Service<hyper::Request<Body>> for ConfigMiddleware<S>
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

    fn call(&mut self, mut req: hyper::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let redis_pool = self.redis_pool.clone();

        async move {
            {
                let extension = req.extensions_mut();

                extension.insert(redis_pool);
            }

            inner.call(req).await
        }
        .boxed()
    }
}
