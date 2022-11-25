use super::service::ConfigMiddleware;
use redis::aio::ConnectionManager;
use tower::Layer;

#[derive(Clone)]
pub struct ConfigSessionLayer(pub ConnectionManager);

impl<S> Layer<S> for ConfigSessionLayer {
    type Service = ConfigMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ConfigMiddleware {
            inner,
            redis_pool: self.0.clone(),
        }
    }
}
