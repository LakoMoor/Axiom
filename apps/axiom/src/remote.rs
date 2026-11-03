//! Remote screen: the real window, mirrored into a browser tab (`--remote`).
//!
//! The app takes a picture of its own window about ten times a second while a browser is watching,
//! serves it from a tiny server on this computer's loopback address, and turns the mouse and keys the
//! browser sends back into the events the window would have received. The screen is the desktop
//! app's own, so it is the same in every detail, and the local models run on this computer as they
//! always do.
//!
//! Safety: the server listens on `127.0.0.1` only; every request must carry the random token that
//! was printed at start (a page that merely guesses the port can't use it); the `Host` header must be
//! the loopback name (so a web page can't reach it through a rebinding trick); requests and bodies are
//! size-capped and time-limited; nothing here writes to disk.

use axiom_platform::sync::Mutex;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const PAGE: &str = include_str!("remote.html");
/// Longest request head and body we read.
const HEAD_MAX: usize = 16 * 1024;
const BODY_MAX: usize = 64 * 1024;
/// Most events taken from one request, and the longest text in one event.
const EVENTS_MAX: usize = 256;
const TEXT_MAX: usize = 256;
/// A picture is taken only while someone has asked for one this recently.
const WATCH_FOR: Duration = Duration::from_secs(4);
const FRAME_EVERY: Duration = Duration::from_millis(90);

#[derive(Default)]
struct State {
    png: Option<Arc<Vec<u8>>>,
    /// The window's size in points, as the last picture was taken.
    size: Vec2,
    inbox: Vec<Event>,
    ctx: Option<egui::Context>,
    watched_at: Option<Instant>,
}

/// What the window and the server share.
#[derive(Default)]
pub struct Shared {
    state: Mutex<State>,
    encoding: AtomicBool,
}

impl Shared {
    fn lock(&self) -> axiom_platform::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A random token, 128 bits as hex. The standard library seeds each `RandomState` from the system.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0u8..2 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u8(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Start the server. Returns the address to open (with the token) and the shared state.
pub fn start(port: u16, token: &str) -> std::io::Result<(String, Arc<Shared>)> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let port = listener.local_addr()?.port();
    let shared = Arc::new(Shared::default());
    let (sh, tok) = (shared.clone(), token.to_string());
    std::thread::Builder::new().name("remote-screen".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let (sh, tok) = (sh.clone(), tok.clone());
            let _ = std::thread::Builder::new().name("remote-conn".into()).spawn(move || {
                let _ = handle(stream, &sh, &tok, port);
            });
        }
    })?;
    Ok((format!("http://127.0.0.1:{port}/?t={token}"), shared))
}

struct Request {
    method: String,
    path: String,
    query: String,
    host: String,
    body: Vec<u8>,
}

/// Read one request: the head up to the blank line, then the body the head announces.
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > HEAD_MAX {
            return None;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(chunk.get(..n)?);
    };
    let head = String::from_utf8_lossy(buf.get(..end)?).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_string(), first.next()?.to_string());
    let (path, query) = target.split_once('?').map_or((target.clone(), String::new()), |(p, q)| (p.to_string(), q.to_string()));
    let (mut host, mut length) = (String::new(), 0usize);
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "host" => host = v.trim().to_string(),
                "content-length" => length = v.trim().parse().ok()?,
                _ => {}
            }
        }
    }
    if length > BODY_MAX {
        return None;
    }
    let mut body = buf.get(end + 4..)?.to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        body.extend_from_slice(chunk.get(..n)?);
    }
    body.truncate(length);
    Some(Request { method, path, query, host, body })
}

/// Whether the query carries the token (compared without stopping at the first difference).
fn token_ok(query: &str, token: &str) -> bool {
    let given = query.split('&').find_map(|kv| kv.strip_prefix("t=")).unwrap_or("");
    given.len() == token.len() && given.bytes().zip(token.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}

/// Whether the `Host` header names this server's own loopback address.
fn host_ok(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}

fn handle(mut stream: TcpStream, shared: &Shared, token: &str, port: u16) -> std::io::Result<()> {
    let Some(req) = read_request(&mut stream) else { return respond(&mut stream, "400 Bad Request", "text/plain", b"bad request") };
    if !host_ok(&req.host, port) || !token_ok(&req.query, token) {
        return respond(&mut stream, "403 Forbidden", "text/plain", b"forbidden");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => respond(&mut stream, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes()),
        ("GET", "/frame.png") => {
            let (png, ctx) = {
                let mut s = shared.lock();
                s.watched_at = Some(Instant::now());
                (s.png.clone(), s.ctx.clone())
            };
            if let Some(c) = ctx {
                c.request_repaint();
            }
            match png {
                Some(p) => respond(&mut stream, "200 OK", "image/png", &p),
                None => respond(&mut stream, "204 No Content", "text/plain", b""),
            }
        }
        ("POST", "/input") => {
            let (size, ctx) = {
                let s = shared.lock();
                (s.size, s.ctx.clone())
            };
            let events = serde_json::from_slice::<Value>(&req.body).map(|v| events_from(&v, size)).unwrap_or_default();
            shared.lock().inbox.extend(events);
            if let Some(c) = ctx {
                c.request_repaint();
            }
            respond(&mut stream, "200 OK", "text/plain", b"ok")
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn modifiers_of(e: &Value) -> Modifiers {
    let flag = |k: &str| e.get(k).and_then(Value::as_bool).unwrap_or(false);
    let (ctrl, meta) = (flag("ctrl"), flag("meta"));
    let mac = cfg!(target_os = "macos");
    Modifiers { alt: flag("alt"), ctrl, shift: flag("shift"), mac_cmd: mac && meta, command: if mac { meta } else { ctrl } }
}

fn button_of(e: &Value) -> PointerButton {
    match e.get("b").and_then(Value::as_u64) {
        Some(1) => PointerButton::Middle,
        Some(2) => PointerButton::Secondary,
        _ => PointerButton::Primary,
    }
}

/// The window events for what a browser sent: `{"events":[{"t":"move","x":0.5,"y":0.2}, …]}` with
/// positions as fractions of the picture. At most `EVENTS_MAX` events, unknown kinds skipped.
pub fn events_from(v: &Value, size: Vec2) -> Vec<Event> {
    let at = |e: &Value| -> Option<Pos2> {
        let (x, y) = (e.get("x")?.as_f64()? as f32, e.get("y")?.as_f64()? as f32);
        (x.is_finite() && y.is_finite()).then(|| Pos2::new(x.clamp(0.0, 1.0) * size.x, y.clamp(0.0, 1.0) * size.y))
    };
    let mut out = Vec::new();
    for e in v.get("events").and_then(Value::as_array).into_iter().flatten().take(EVENTS_MAX) {
        let mods = modifiers_of(e);
        match e.get("t").and_then(Value::as_str).unwrap_or("") {
            "move" => out.extend(at(e).map(Event::PointerMoved)),
            "leave" => out.push(Event::PointerGone),
            "down" | "up" => {
                let pressed = e.get("t").and_then(Value::as_str) == Some("down");
                out.extend(at(e).map(|pos| Event::PointerButton { pos, button: button_of(e), pressed, modifiers: mods }));
            }
            "wheel" => {
                let num = |k: &str| e.get(k).and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0).clamp(-2000.0, 2000.0) as f32;
                // A browser's positive deltaY scrolls down, which moves the content up.
                out.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(-num("dx"), -num("dy")),
                    phase: egui::TouchPhase::Move,
                    modifiers: mods,
                });
            }
            "key" => {
                let pressed = e.get("down").and_then(Value::as_bool).unwrap_or(true);
                if let Some(key) = e.get("name").and_then(Value::as_str).and_then(Key::from_name) {
                    out.push(Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: e.get("repeat").and_then(Value::as_bool).unwrap_or(false),
                        modifiers: mods,
                    });
                }
            }
            "text" => {
                if let Some(s) = e.get("s").and_then(Value::as_str) {
                    let s: String = s.chars().filter(|c| !c.is_control()).take(TEXT_MAX).collect();
                    if !s.is_empty() {
                        out.push(Event::Text(s));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Wraps the app: the same window, plus the mirror.
pub struct Mirror<A> {
    pub inner: A,
    shared: Arc<Shared>,
    last_shot: Instant,
}

impl<A> Mirror<A> {
    pub fn new(inner: A, shared: Arc<Shared>) -> Self {
        Self { inner, shared, last_shot: Instant::now() }
    }
}

fn encode_png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity(w.checked_mul(h)?.checked_mul(4)?);
    for p in &img.pixels {
        raw.extend_from_slice(&p.to_array());
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().ok()?;
    writer.write_image_data(&raw).ok()?;
    writer.finish().ok()?;
    Some(out)
}

impl<A: eframe::App> eframe::App for Mirror<A> {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // The picture asked for last frame arrives as an event; turn it into a PNG off this thread.
        let shots: Vec<Arc<egui::ColorImage>> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });
        if let Some(img) = shots.into_iter().next_back()
            && !self.shared.encoding.swap(true, Ordering::SeqCst)
        {
            let sh = self.shared.clone();
            let size = ctx.content_rect().size();
            let spawned = std::thread::Builder::new().name("remote-png".into()).spawn(move || {
                let png = encode_png(&img).map(Arc::new);
                {
                    let mut s = sh.lock();
                    if png.is_some() {
                        s.png = png;
                    }
                    s.size = size;
                }
                sh.encoding.store(false, Ordering::SeqCst);
            });
            if spawned.is_err() {
                self.shared.encoding.store(false, Ordering::SeqCst);
            }
        }
        self.inner.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.ui(ui, frame);
        let ctx = ui.ctx().clone();
        let watching = {
            let mut s = self.shared.lock();
            if s.ctx.is_none() {
                s.ctx = Some(ctx.clone());
            }
            s.size = ctx.content_rect().size();
            s.watched_at.is_some_and(|t| t.elapsed() < WATCH_FOR)
        };
        if watching {
            if self.last_shot.elapsed() >= FRAME_EVERY {
                self.last_shot = Instant::now();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint_after(FRAME_EVERY);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let events = std::mem::take(&mut self.shared.lock().inbox);
        if !events.is_empty() {
            raw.events.extend(events);
            ctx.request_repaint();
        }
        self.inner.raw_input_hook(ctx, raw);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.inner.save(storage);
    }

    fn on_exit(&mut self) {
        self.inner.on_exit();
    }

    fn auto_save_interval(&self) -> Duration {
        self.inner.auto_save_interval()
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.inner.clear_color(visuals)
    }

    fn persist_egui_memory(&self) -> bool {
        self.inner.persist_egui_memory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn get(port: u16, host: &str, target: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn post(port: u16, target: &str, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "POST {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn tokens_are_long_and_different_each_time() {
        let (a, b) = (new_token(), new_token());
        assert_eq!((a.len(), a.chars().all(|c| c.is_ascii_hexdigit())), (32, true));
        assert_ne!(a, b);
    }

    #[test]
    fn only_the_token_and_the_loopback_name_get_in() {
        assert!(token_ok("x=1&t=abc", "abc") && !token_ok("t=abd", "abc") && !token_ok("t=ab", "abc") && !token_ok("", "abc"));
        assert!(host_ok("127.0.0.1:5000", 5000) && host_ok("localhost:5000", 5000));
        assert!(!host_ok("evil.example:5000", 5000) && !host_ok("127.0.0.1:5001", 5000) && !host_ok("", 5000));
    }

    #[test]
    fn the_server_checks_token_and_host_serves_the_page_and_takes_input() {
        let (url, shared) = start(0, "secret").unwrap();
        let port: u16 = url.split(':').nth(2).and_then(|r| r.split('/').next()).and_then(|p| p.parse().ok()).unwrap();
        let host = format!("127.0.0.1:{port}");
        assert!(get(port, &host, "/").starts_with("HTTP/1.1 403"), "no token");
        assert!(get(port, &host, "/?t=wrong").starts_with("HTTP/1.1 403"), "wrong token");
        assert!(get(port, "evil.example", "/?t=secret").starts_with("HTTP/1.1 403"), "wrong host");
        let page = get(port, &host, "/?t=secret");
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("<canvas"), "{page}");
        assert!(get(port, &host, "/frame.png?t=secret").starts_with("HTTP/1.1 204"), "no picture yet");
        assert!(shared.lock().watched_at.is_some(), "asking for a picture marks someone as watching");
        shared.lock().size = Vec2::new(200.0, 100.0);
        let body = json!({"events": [{"t": "move", "x": 0.5, "y": 0.5}, {"t": "text", "s": "hi\n"}]}).to_string();
        assert!(post(port, "/input?t=secret", &body).starts_with("HTTP/1.1 200"));
        assert!(post(port, "/input?t=nope", &body).starts_with("HTTP/1.1 403"));
        let inbox = std::mem::take(&mut shared.lock().inbox);
        assert_eq!(inbox, vec![Event::PointerMoved(Pos2::new(100.0, 50.0)), Event::Text("hi".into())]);
        assert!(get(port, &host, "/nothing?t=secret").starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn a_body_over_the_cap_or_junk_is_refused_without_harm() {
        let (_, shared) = start(0, "secret").unwrap();
        assert!(shared.lock().inbox.is_empty());
        let size = Vec2::new(100.0, 100.0);
        assert!(events_from(&json!("junk"), size).is_empty());
        assert!(events_from(&json!({"events": "x"}), size).is_empty());
        assert!(events_from(&json!({"events": [{"t": "move", "x": "a", "y": 1}, {"t": "nope"}, 5]}), size).is_empty());
        let many: Vec<Value> = (0..1000).map(|_| json!({"t": "leave"})).collect();
        assert_eq!(events_from(&json!({ "events": many }), size).len(), EVENTS_MAX);
    }

    #[test]
    fn browser_input_becomes_window_events() {
        let size = Vec2::new(400.0, 200.0);
        let v = json!({"events": [
            {"t": "down", "b": 2, "x": 2.0, "y": -1.0},
            {"t": "up", "x": 0.25, "y": 0.5},
            {"t": "wheel", "dx": 0, "dy": 30},
            {"t": "key", "name": "Enter", "down": true, "shift": true},
            {"t": "key", "name": "NotAKey"},
            {"t": "text", "s": "a\u{7}b"},
        ]});
        let e = events_from(&v, size);
        assert_eq!(e.len(), 5);
        assert!(
            matches!(&e[0], Event::PointerButton { pos, button: PointerButton::Secondary, pressed: true, .. } if *pos == Pos2::new(400.0, 0.0)),
            "clamped to the window: {:?}",
            e[0]
        );
        assert!(matches!(&e[1], Event::PointerButton { pos, pressed: false, .. } if *pos == Pos2::new(100.0, 100.0)));
        assert!(matches!(&e[2], Event::MouseWheel { delta, .. } if *delta == Vec2::new(0.0, -30.0)), "down in a browser moves the content up");
        assert!(matches!(&e[3], Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.shift));
        assert_eq!(e[4], Event::Text("ab".into()), "control characters are dropped");
    }

    #[test]
    fn a_picture_becomes_a_png() {
        let img = egui::ColorImage::filled([4, 3], egui::Color32::from_rgb(10, 20, 30));
        let png = encode_png(&img).unwrap();
        assert_eq!(png.get(..8), Some(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a][..]));
    }
}
