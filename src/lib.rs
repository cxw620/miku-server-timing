//! Miku's Server-Timing middleware for Axum

use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Instant,
};

use http::{header::Entry as HeaderEntry, HeaderName, HeaderValue, Request, Response};
use macro_toolset::{
    str_concat,
    string::{NumStr, StringExtT},
};
use pin_project_lite::pin_project;

#[derive(Debug, Clone)]
/// A middleware that will add a Server-Timing header to the response.
pub struct ServerTimingLayer<'a> {
    /// The service name.
    app: &'a str,

    /// An optional description of the service.
    description: Option<&'a str>,
}

impl<'a> ServerTimingLayer<'a> {
    #[inline]
    /// Creates a new `ServerTimingLayer` with the given service name.
    pub const fn new(app: &'a str) -> Self {
        ServerTimingLayer {
            app,
            description: None,
        }
    }

    #[inline]
    /// Adds a description to the service name.
    pub const fn with_description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }
}

impl<'a, S> tower_layer::Layer<S> for ServerTimingLayer<'a> {
    type Service = ServerTimingService<'a, S>;

    fn layer(&self, service: S) -> Self::Service {
        ServerTimingService {
            service,
            app: self.app,
            description: self.description,
        }
    }
}

#[derive(Debug, Clone)]
/// A service that will add a Server-Timing header to the response.
pub struct ServerTimingService<'a, S> {
    /// The service to wrap.
    service: S,

    /// The service name.
    app: &'a str,

    /// An optional description of the service.
    description: Option<&'a str>,
}

impl<'a, S, ReqBody, ResBody> tower_service::Service<Request<ReqBody>>
    for ServerTimingService<'a, S>
where
    S: tower_service::Service<Request<ReqBody>, Response = Response<ResBody>>,
    ResBody: Default,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<'a, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        ResponseFuture {
            inner: self.service.call(req),
            request_time: Instant::now(),
            app: self.app,
            description: self.description,
        }
    }
}

pin_project! {
    /// A future that will add a Server-Timing header to the response.
    pub struct ResponseFuture<'a, F> {
        #[pin]
        inner: F,
        request_time: Instant,
        app: &'a str,
        description: Option<&'a str>,
    }
}

const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");

impl<F, B, E> Future for ResponseFuture<'_, F>
where
    F: Future<Output = Result<Response<B>, E>>,
    B: Default,
{
    type Output = Result<Response<B>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let mut response: Response<B> = ready!(this.inner.poll(cx))?;

        match response.headers_mut().try_entry(SERVER_TIMING) {
            Ok(entry) => {
                let new_server_timing_content = (
                    this.app,
                    ";",
                    this.description.with_prefix("desc=\"").with_suffix("\";"),
                    "dur=",
                    NumStr::new_default(this.request_time.elapsed().as_secs_f32() * 1000.0)
                        .set_resize_len::<1>(),
                );

                match entry {
                    HeaderEntry::Occupied(mut val) => {
                        val.insert(
                            HeaderValue::from_str(&str_concat!(
                                new_server_timing_content,
                                val.get().to_str().with_prefix(", ")
                            ))
                            .unwrap(),
                        );
                    }
                    HeaderEntry::Vacant(val) => {
                        val.insert(
                            HeaderValue::from_str(&str_concat!(new_server_timing_content)).unwrap(),
                        );
                    }
                }
            }
            Err(_e) => {
                #[cfg(feature = "feat-tracing")]
                tracing::error!("Failed to add `server-timing` header: {_e:?}");
                // header name was invalid (it wasn't) or too many headers (just
                // give up).
            }
        };

        Poll::Ready(Ok(response))
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use axum::{routing::get, Router};
    use http::{HeaderMap, HeaderValue};

    use super::ServerTimingLayer;

    #[test]
    fn service_name() {
        let name = "svc1";
        let obj = ServerTimingLayer::new(name);
        assert_eq!(obj.app, name);
    }

    #[test]
    fn service_desc() {
        let name = "svc1";
        let desc = "desc1";
        let obj = ServerTimingLayer::new(name).with_description(desc);
        assert_eq!(obj.app, name);
        assert_eq!(obj.description, Some(desc));
    }

    #[tokio::test]
    async fn axum_test() {
        let name = "svc1";
        let app = Router::new()
            .route(
                "/",
                get(|| async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    ""
                }),
            )
            .layer(ServerTimingLayer::new(name));

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();

        tokio::spawn(async move { axum::serve(listener, app.into_make_service()).await });

        let _ = tokio::task::spawn_blocking(|| {
            let headers = minreq::get("http://localhost:3001/")
                .send()
                .unwrap()
                .headers;

            let hdr = headers.get("server-timing");
            assert!(
                hdr.is_some(),
                "Cannot find `server-timing` from: {headers:#?}"
            );

            let val: f32 = hdr.unwrap()[9..].parse().unwrap();
            assert!(
                (100f32..300f32).contains(&val),
                "Invalid `server-timing` from: {headers:#?}"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn support_existing_header() {
        let name = "svc1";
        let app = Router::new()
            .route(
                "/",
                get(|| async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let mut hdr = HeaderMap::new();
                    hdr.insert("server-timing", HeaderValue::from_static("inner;dur=23"));
                    (hdr, "")
                }),
            )
            .layer(ServerTimingLayer::new(name));

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3003").await.unwrap();
        tokio::spawn(async { axum::serve(listener, app.into_make_service()).await });

        let _ = tokio::task::spawn_blocking(|| {
            let headers = minreq::get("http://localhost:3003/")
                .send()
                .unwrap()
                .headers;

            let hdr = headers.get("server-timing").unwrap();
            assert!(hdr.contains("svc1"));
            assert!(hdr.contains("inner"));
            println!("{hdr:?}");
        })
        .await;
    }
}
