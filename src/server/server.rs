use crate::{Decoded, HttpServer, HttpService, Request, Response, RouteMatcher};
use std::io;

pub type Middleware =
    Box<dyn Fn(&Request, &mut Response) -> io::Result<()> + Send + Sync + 'static>;

pub type RouteHandler =
    Box<dyn Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static>;

#[derive(Clone)]
pub struct Server {
    route_handlers: RouteMatcher,
}

impl Server {
    pub fn new() -> Self {
        Server {
            route_handlers: RouteMatcher::new(),
        }
    }

    pub fn listen(&mut self, addr: &str) -> io::Result<()> {
        may::config().set_workers(8);
        let server = HttpServer(self.clone()).start(addr)?;
        server.wait();
        Ok(())
    }

    pub fn add_route_handler<F>(&mut self, method: &str, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.route_handlers
            .add_route(method, path, Box::new(handler));
    }

    pub fn get<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("GET", path, handler);
    }

    pub fn post<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("POST", path, handler);
    }

    pub fn put<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("PUT", path, handler);
    }

    pub fn delete<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("DELETE", path, handler);
    }

    pub fn head<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("HEAD", path, handler);
    }

    pub fn options<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("OPTIONS", path, handler);
    }

    pub fn trace<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("TRACE", path, handler);
    }

    pub fn connect<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("CONNECT", path, handler);
    }

    pub fn patch<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request, &mut Response) -> io::Result<()> + Send + Sync + 'static,
    {
        self.add_route_handler("PATCH", path, handler);
    }
}

impl HttpService for Server {
    fn handler(&mut self, req: Decoded, res: &mut Response) -> io::Result<()> {
        // Run route handler if exists
        let method = req.raw().method.unwrap();
        let url = req.raw().path.unwrap();
        let body = req.body();

        if let Some(matched_route) = self.route_handlers.match_route(method, url) {
            let parameters = matched_route.parameters;
            let url_parameters = matched_route.url_parameters;
            let req = Request::new(req, parameters, url_parameters, body);
            (matched_route.handler)(req, res)
        } else {
            // No route handler found, return 404
            res.status_code(404, "Not Found");
            Ok(())
        }
    }
}
