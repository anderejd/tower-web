use service::ResponseBody;
use {Resource, Service};

use bytes::Bytes;
use http;
use hyper;
use hyper::server::{Http, Service as HyperService};

use tokio;
use tokio::net::TcpListener;
use tokio::prelude::*;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

struct Lift<T: Resource> {
    inner: Service<T>,
}

struct LiftBody<T>(T);

impl<T> Lift<T>
where
    T: Resource,
{
    fn new(inner: Service<T>) -> Self {
        Lift { inner }
    }
}

impl<T> Stream for LiftBody<T>
where
    T: Stream<Item = Bytes>,
{
    type Item = Bytes;
    type Error = hyper::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.0.poll() {
            Ok(v) => Ok(v),
            Err(_) => unimplemented!(),
        }
    }
}

impl<T> HyperService for Lift<T>
where
    T: Resource,
    /*
where T: tower::Service<Request = http::Request<String>,
                        Response = http::Response<String>> + Clone + Send + 'static,
      T::Future: Send,
      */
{
    type Request = hyper::Request;
    type Response = hyper::Response<LiftBody<ResponseBody<T>>>;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error> + Send>;

    fn call(&self, req: Self::Request) -> Self::Future {
        use tower_service::Service;

        let req: http::Request<_> = req.into();
        let (head, body) = req.into_parts();

        let mut inner = self.inner.clone();

        let fut = body.concat2()
            .and_then(move |body| {
                // Convert the body to a string
                let body = String::from_utf8(body.to_vec()).unwrap();

                // Rebuild the request
                let req = http::Request::from_parts(head, body);

                // Call the inner service
                inner.call(req).map_err(|_| unimplemented!())
            })
            .map(|response| response.map(LiftBody).into());

        Box::new(fut)
    }
}

/// Run a service
pub fn run<T>(addr: &SocketAddr, service: Service<T>) -> io::Result<()>
where
    T: Resource,
    /*
where T: tower::Service<Request = http::Request<String>,
                       Response = http::Response<String>> + Clone + Send + 'static,
      T::Future: Send,
      */
{
    let listener = TcpListener::bind(addr)?;
    let http = Arc::new(Http::<String>::new());

    tokio::run({
        listener
            .incoming()
            .map_err(|e| println!("failed to accept socket; err = {:?}", e))
            .for_each(move |socket| {
                let h = http.clone();

                tokio::spawn({
                    let service = Lift::new(service.clone());

                    h.serve_connection(socket, service)
                        .map(|_| ())
                        .map_err(|e| {
                            println!("failed to serve connection; err={:?}", e);
                        })
                })
            })
    });

    Ok(())
}
