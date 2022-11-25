use crate::app::util::error::ServiceError;
use cookie::{Cookie, CookieJar};
use futures::future::{BoxFuture, FutureExt as _};
use hyper::Body;
use redis::aio::ConnectionManager;
// use redis::aio::ConnectionManager;
use time::Duration;
use tonic::body::BoxBody;
use tower::{BoxError, Service};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CookieMiddleware<S> {
    pub inner: S,
}

#[derive(Debug, Clone)]
pub struct CookieSessionContainer(pub Option<CookieSession>);

#[derive(Debug, Clone)]
pub struct CookieSession {
    pub sid: String,
    pub uid: Uuid,
}

impl<S> Service<hyper::Request<Body>> for CookieMiddleware<S>
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

    fn call(&mut self, mut req: hyper::Request<Body>) -> Self::Future {
        // This is necessary because tonic internally uses `tower::buffer::Buffer`.
        // See https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        // for details on why this is necessary
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        async move {
            inspect_request_metadata(&mut req).await?;

            insert_empty_extension(&mut req);

            inner.call(req).await
        }
        .boxed()
    }
}

fn box_into_error<T, E>(error: E) -> Result<T, BoxError>
where
    E: Into<ServiceError>,
{
    Err(Box::new(error.into()))
}

fn insert_empty_extension(req: &mut hyper::Request<Body>) {
    let extension = req.extensions_mut();

    if extension.get::<CookieSessionContainer>().is_none() {
        extension.insert(CookieSessionContainer(None));
    }
}

async fn inspect_request_metadata(req: &mut hyper::Request<Body>) -> Result<(), BoxError> {
    let header = req.headers().get("cookie").map(|header| {
        header.to_str().map(|header| {
            let mut raw_cookies = header.split("; ").map(String::from);

            let mut cookie_jar = CookieJar::new();

            raw_cookies
                .try_for_each(|raw_cookie| -> Result<(), cookie::ParseError> {
                    let cookie = Cookie::parse(raw_cookie)?;

                    cookie_jar.add(cookie);

                    Ok(())
                })
                .map(|_| cookie_jar)
        })
    });

    let session = req
        .headers()
        .get("Session")
        .map(|header| header.to_str().map(|header| header.to_string()));

    let redis_pool = {
        let extension = req.extensions();

        extension.get::<ConnectionManager>()
    }
    .cloned();

    match (header, session, redis_pool) {
        (Some(Ok(Ok(cookie_jar))), _, Some(mut redis_pool)) => {
            if let Some(cookie) = cookie_jar.get("session") {
                let record = redis::cmd("GETEX")
                    .arg(cookie.value())
                    .arg("EX")
                    .arg(Duration::hours(24).whole_seconds())
                    .query_async::<_, Option<String>>(&mut redis_pool)
                    .await;

                match record.map(|uid| uid.map(|uid| Uuid::parse_str(&uid))) {
                    Ok(Some(Ok(uid))) => {
                        let extension = req.extensions_mut();

                        extension.insert(CookieSessionContainer(Some(CookieSession {
                            sid: cookie.value().to_string(),
                            uid,
                        })));

                        Ok(())
                    }
                    Ok(Some(Err(e))) => box_into_error(e)?,
                    Ok(None) => box_into_error(ServiceError::BadCredential)?,
                    Err(e) => box_into_error(e)?,
                }
            } else {
                Ok(())
            }
        }
        (_, Some(Ok(sid)), Some(mut redis_pool)) => {
            let record = redis::cmd("GETEX")
                .arg(&sid)
                .arg("EX")
                .arg(Duration::hours(24).whole_seconds())
                .query_async::<_, Option<String>>(&mut redis_pool)
                .await;

            match record.map(|uid| uid.map(|uid| Uuid::parse_str(&uid))) {
                Ok(Some(Ok(uid))) => {
                    let extension = req.extensions_mut();

                    extension.insert(CookieSessionContainer(Some(CookieSession { sid, uid })));

                    Ok(())
                }
                Ok(Some(Err(e))) => box_into_error(e)?,
                Ok(None) => box_into_error(ServiceError::BadCredential)?,
                Err(e) => box_into_error(e)?,
            }
        }
        (Some(Ok(Err(e))), _, _) => box_into_error(e)?,
        (Some(Err(e)), _, _) => box_into_error(e)?,
        (_, Some(Err(e)), _) => box_into_error(e)?,
        (_, _, None) => box_into_error(ServiceError::MiddlewareNotSet("config"))?,
        // (None, _) => box_into_error(GeekyRepercussion::HttpHeaderNotFound)?,
        (_, None, _) => Ok(()),
    }
}
