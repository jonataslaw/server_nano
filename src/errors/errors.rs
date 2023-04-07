use serde_json::Error as JsonError;
use std::fmt;

use std::str::Utf8Error;

#[derive(Debug)]
pub enum RequestError {
    JsonError(JsonError),
    Utf8Error(Utf8Error),
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RequestError::JsonError(e) => write!(f, "JSON Error: {}", e),
            RequestError::Utf8Error(e) => write!(f, "UTF-8 Error: {}", e),
        }
    }
}

impl std::error::Error for RequestError {}

impl From<JsonError> for RequestError {
    fn from(e: JsonError) -> Self {
        RequestError::JsonError(e)
    }
}

impl From<Utf8Error> for RequestError {
    fn from(e: Utf8Error) -> Self {
        RequestError::Utf8Error(e)
    }
}
