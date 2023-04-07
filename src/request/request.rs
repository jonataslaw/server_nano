use std::collections::HashMap;

use std::str;

use crate::errors::errors::RequestError;

use super::decode::Decoded;

pub struct Request<'a, 'header> {
    req: Decoded<'header, 'a>,
    pub parameters: HashMap<String, String>,
    pub url_parameters: HashMap<String, String>,
    body: &'a [u8],
}

impl<'a, 'header> Request<'a, 'header> {
    pub fn new(
        req: Decoded<'header, 'a>,
        parameters: HashMap<String, String>,
        url_parameters: HashMap<String, String>,
        body: &'a [u8],
    ) -> Self {
        Request {
            req,
            parameters,
            url_parameters,
            body,
        }
    }

    pub fn parameter(&self, key: &str) -> Option<&str> {
        self.parameters.get(key).map(|s| s.as_str())
    }

    pub fn url_parameter(&self, key: &str) -> Option<&str> {
        self.url_parameters.get(key).map(|s| s.as_str())
    }

    pub fn body(&self) -> &'a [u8] {
        &self.body
    }

    pub fn str_body(&self) -> Result<&str, str::Utf8Error> {
        str::from_utf8(&self.body)
    }

    pub fn json_body(&self) -> Result<serde_json::Value, RequestError> {
        let body_str = std::str::from_utf8(&self.body)?;
        serde_json::from_str(body_str).map_err(RequestError::from)
    }

    pub fn method(&self) -> &str {
        return self.req.raw().method.unwrap();
    }

    pub fn path(&self) -> &str {
        return self.req.raw().path.unwrap();
    }

    pub fn keep_alive(&self) -> bool {
        return self.headers().iter().any(|header| {
            header.name.eq_ignore_ascii_case("connection")
                && std::str::from_utf8(header.value).ok() == Some("keep-alive")
        });
    }

    pub fn version(&self) -> u8 {
        return self.req.raw().version.unwrap();
    }

    pub fn headers(&self) -> &[httparse::Header<'_>] {
        return self.req.raw().headers;
    }
}
