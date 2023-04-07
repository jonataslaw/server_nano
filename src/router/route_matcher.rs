use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::RouteHandler;

#[derive(Clone)]
pub struct RouteMatcher {
    routes: HashSet<RouteNode>,
}

struct RouteNode {
    method: String,
    path: String,
    segments: Vec<Segment>,
    handler: Arc<RouteHandler>,
}

impl Clone for RouteNode {
    fn clone(&self) -> Self {
        RouteNode {
            method: self.method.clone(),
            path: self.path.clone(),
            segments: self.segments.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum Segment {
    Static(String),
    Parameter(String),
    Wildcard,
}

impl Hash for RouteNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
}

impl PartialEq for RouteNode {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl Eq for RouteNode {}

pub struct MatchedRoute {
    pub method: String,
    pub path: String,
    pub parameters: HashMap<String, String>,
    pub url_parameters: HashMap<String, String>,
    pub handler: Arc<RouteHandler>,
}

impl RouteMatcher {
    pub fn new() -> RouteMatcher {
        RouteMatcher {
            routes: HashSet::new(),
        }
    }

    pub fn add_route(&mut self, method: &str, path: &str, handler: RouteHandler) {
        let segments = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with(':') {
                    Segment::Parameter(s[1..].to_string())
                } else if s == "*" {
                    Segment::Wildcard
                } else {
                    Segment::Static(s.to_string())
                }
            })
            .collect::<Vec<_>>();
        self.routes.insert(RouteNode {
            method: method.to_string(),
            path: path.to_string(),
            segments,
            handler: Arc::new(handler),
        });
    }

    pub fn match_route(&self, method: &str, url: &str) -> Option<MatchedRoute> {
        let (path, query_string) = url.split_at(url.find('?').unwrap_or_else(|| url.len()));
        let segments = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        for route in &self.routes {
            if let Some(parameters) = route.match_segments(&segments) {
                let url_parameters = query_string
                    .trim_start_matches('?')
                    .split('&')
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        let mut parts = s.split('=');
                        (
                            parts.next().unwrap().to_string(),
                            parts.next().unwrap_or("").to_string(),
                        )
                    })
                    .collect::<HashMap<_, _>>();

                if &route.method != method && route.method != "*" {
                    continue;
                }
                return Some(MatchedRoute {
                    method: route.method.clone(),
                    path: route.path.clone(),
                    parameters,
                    url_parameters,
                    handler: Arc::clone(&route.handler),
                });
            }
        }

        None
    }
}

impl RouteNode {
    fn match_segments(&self, segments: &[&str]) -> Option<HashMap<String, String>> {
        if self.segments.len() != segments.len() && !self.segments.contains(&Segment::Wildcard) {
            return None;
        }

        let mut parameters = HashMap::new();
        let mut wildcard = false;

        for (route_segment, segment) in self.segments.iter().zip(segments.iter()) {
            match route_segment {
                Segment::Static(s) => {
                    if s != segment {
                        return None;
                    }
                }
                Segment::Parameter(param) => {
                    parameters.insert(param.clone(), (*segment).to_string());
                }
                Segment::Wildcard => {
                    wildcard = true;
                    break;
                }
            }
        }

        if wildcard {
            Some(parameters)
        } else if self.segments.len() == segments.len() {
            Some(parameters)
        } else {
            None
        }
    }
}
