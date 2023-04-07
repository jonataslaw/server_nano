#[macro_use]
extern crate log;

pub mod server {
    pub mod server;
}

mod http {
    pub mod http_server;
}

mod request {
    pub mod decode;
    pub mod request;
}

mod response {
    pub mod date;
    pub mod response;
}

mod router {
    pub mod route_matcher;
}

mod errors {
    pub mod errors;
}

use http::http_server::{HttpServer, HttpService};
use request::decode::Decoded;
use request::request::Request;
use response::response::Response;
use router::route_matcher::RouteMatcher;

pub use server::server::{Middleware, RouteHandler, Server};

pub use serde_json::json;
