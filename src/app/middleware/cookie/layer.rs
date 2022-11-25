use super::service::CookieMiddleware;
use tower::Layer;

#[derive(Debug, Clone)]
pub struct CookieSessionLayer;

impl<S> Layer<S> for CookieSessionLayer {
    type Service = CookieMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CookieMiddleware { inner }
    }
}
