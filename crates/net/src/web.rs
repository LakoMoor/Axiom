//! The browser backend: the part of `ureq`'s API that Axiom uses, on a synchronous XMLHttpRequest.

use std::io::Read;
use std::time::Duration;
use wasm_bindgen::JsValue;
use web_sys::XmlHttpRequest;

/// What went wrong with a request.
#[derive(Debug)]
pub enum Error {
    /// The request took too long (the browser gave up).
    Timeout(String),
    /// The browser refused or couldn't reach the server (offline, blocked by CORS, mixed content…).
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Timeout(m) | Error::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

/// Settings for an [`Agent`]. Timeouts can't be set on a synchronous request in a page, so they are
/// accepted and ignored.
#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder;

impl ConfigBuilder {
    pub fn http_status_as_error(self, _on: bool) -> Self {
        self
    }
    pub fn user_agent(self, _agent: &str) -> Self {
        self
    }
    pub fn timeout_connect(self, _d: Option<Duration>) -> Self {
        self
    }
    pub fn timeout_recv_response(self, _d: Option<Duration>) -> Self {
        self
    }
    pub fn timeout_recv_body(self, _d: Option<Duration>) -> Self {
        self
    }
    pub fn timeout_global(self, _d: Option<Duration>) -> Self {
        self
    }
    pub fn build(self) -> Config {
        Config
    }
}

#[derive(Debug, Clone, Default)]
pub struct Config;

impl Config {
    pub fn new_agent(self) -> Agent {
        Agent
    }
}

/// Makes requests.
#[derive(Debug, Clone, Default)]
pub struct Agent;

impl Agent {
    pub fn config_builder() -> ConfigBuilder {
        ConfigBuilder
    }
    pub fn get(&self, url: &str) -> Request {
        Request::new("GET", url)
    }
    pub fn post(&self, url: &str) -> Request {
        Request::new("POST", url)
    }
}

/// A request being built.
#[derive(Debug, Clone)]
pub struct Request {
    method: &'static str,
    url: String,
    headers: Vec<(String, String)>,
}

impl Request {
    fn new(method: &'static str, url: &str) -> Self {
        Request { method, url: url.to_string(), headers: Vec::new() }
    }

    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.headers.push((name.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    /// Send a request with no body.
    pub fn call(self) -> Result<Response, Error> {
        self.run(None)
    }

    /// Send a request with a text body.
    pub fn send(self, body: impl Into<String>) -> Result<Response, Error> {
        let body = body.into();
        self.run(Some(&body))
    }

    fn run(self, body: Option<&str>) -> Result<Response, Error> {
        let js = |e: JsValue| Error::Other(e.as_string().unwrap_or_else(|| format!("{e:?}")));
        let xhr = XmlHttpRequest::new().map_err(js)?;
        xhr.open_with_async(self.method, &self.url, false).map_err(js)?;
        for (k, v) in &self.headers {
            // A header the page may not set (User-Agent, Host…) is skipped by the browser.
            let _ = xhr.set_request_header(k, v);
        }
        match body {
            Some(b) => xhr.send_with_opt_str(Some(b)),
            None => xhr.send(),
        }
        .map_err(|e| {
            let m = e.as_string().unwrap_or_else(|| "the request was blocked or couldn't connect".to_string());
            if m.to_ascii_lowercase().contains("timeout") { Error::Timeout(m) } else { Error::Other(m) }
        })?;
        let status = xhr.status().map_err(js)?;
        // A status of 0 is the browser saying it never got an answer (offline, CORS, blocked).
        if status == 0 {
            return Err(Error::Other(format!("no answer from {} (offline, or the server doesn't allow this page's origin)", self.url)));
        }
        let text = xhr.response_text().map_err(js)?.unwrap_or_default();
        let headers = xhr
            .get_all_response_headers()
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_ascii_lowercase(), HeaderValue(v.trim().to_string()))))
            .collect();
        Ok(Response { status, headers: Headers(headers), body: Body(text) })
    }
}

/// An HTTP status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status(u16);

impl Status {
    pub fn as_u16(self) -> u16 {
        self.0
    }
    pub fn is_success(self) -> bool {
        (200..300).contains(&self.0)
    }
}

/// One header's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderValue(String);

impl HeaderValue {
    pub fn to_str(&self) -> Result<&str, std::convert::Infallible> {
        Ok(&self.0)
    }
}

/// The headers of an answer; names are looked up without regard to case.
#[derive(Debug, Clone, Default)]
pub struct Headers(Vec<(String, HeaderValue)>);

impl Headers {
    pub fn get(&self, name: &str) -> Option<&HeaderValue> {
        let name = name.to_ascii_lowercase();
        self.0.iter().find(|(k, _)| *k == name).map(|(_, v)| v)
    }
}

/// An answer.
#[derive(Debug)]
pub struct Response<B = Body> {
    status: u16,
    headers: Headers,
    body: B,
}

impl<B> Response<B> {
    pub fn status(&self) -> Status {
        Status(self.status)
    }
    pub fn headers(&self) -> &Headers {
        &self.headers
    }
    pub fn into_body(self) -> B {
        self.body
    }
    pub fn body_mut(&mut self) -> &mut B {
        &mut self.body
    }
}

/// The body of an answer, already received in full.
#[derive(Debug)]
pub struct Body(String);

impl Body {
    pub fn read_to_string(&mut self) -> Result<String, Error> {
        Ok(std::mem::take(&mut self.0))
    }
    pub fn into_reader(self) -> impl Read + Send {
        std::io::Cursor::new(self.0.into_bytes())
    }
    /// Read with a limit on how much is taken.
    pub fn with_config(&mut self) -> BodyConfig<'_> {
        BodyConfig { body: self, limit: u64::MAX }
    }
}

/// How much of a [`Body`] to read.
pub struct BodyConfig<'a> {
    body: &'a mut Body,
    limit: u64,
}

impl BodyConfig<'_> {
    pub fn limit(mut self, bytes: u64) -> Self {
        self.limit = bytes;
        self
    }
    fn taken(self) -> Vec<u8> {
        let all = std::mem::take(&mut self.body.0).into_bytes();
        let cut = usize::try_from(self.limit).map_or(all.len(), |n| n.min(all.len()));
        all.into_iter().take(cut).collect()
    }
    pub fn read_to_string(self) -> Result<String, Error> {
        String::from_utf8(self.taken()).map_err(|e| Error::Other(format!("the answer isn't text: {e}")))
    }
    pub fn reader(self) -> impl Read + Send {
        std::io::Cursor::new(self.taken())
    }
}

/// `ureq::http` on computers; here only the answer type.
pub mod http {
    pub type Response<B> = crate::Response<B>;
}
