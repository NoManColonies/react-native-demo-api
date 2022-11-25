use crate::app::{middleware::cookie::service::CookieSessionContainer, util::error::ServiceError};
use tonic::{Request, Status};

pub fn cookie_session_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let extension = req.extensions();

    if let Some(container) = extension.get::<CookieSessionContainer>() {
        if container.0.is_some() {
            Ok(req)
        } else {
            Err(ServiceError::BadCredential.into())
        }
    } else {
        Err(ServiceError::MiddlewareNotSet("cookie").into())
    }
}
