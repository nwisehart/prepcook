//! Host-based reverse proxy exposing the in-container lavish-axi review
//! server to remote browsers.
//!
//! The lavish-axi server binds 127.0.0.1:<LAVISH_AXI_PORT> (loopback only),
//! which is fine when the orchestrator runs on the reviewer's own machine but
//! unreachable when it runs on a hosted runner. Rather than changing how
//! lavish binds (or publishing per-session ports), the app server — which
//! shares the network namespace with lavish and is already published —
//! forwards any request whose Host header matches `LAVISH_PUBLIC_BASE` to the
//! loopback lavish server. The hosted reverse proxy (caddy) points a
//! dedicated hostname (e.g. lavish.prepcook.example.com) at the app
//! port, giving lavish its own origin so its absolute asset/API paths work
//! unmodified.
//!
//! Unset LAVISH_PUBLIC_BASE (local dev / desktop) makes this a no-op.
//! Exposure matches the rest of the app: the hostname is only reachable
//! over the VPN, and lavish sessions were equally unauthenticated when
//! opened via their localhost URLs.
//!
//! This proxy is also the same-origin boundary. Lavish (0.1.50 and later)
//! refuses a prompt submission whose `Origin` does not match the origin it
//! computes from the `Host` it receives - which, behind this proxy, is the
//! loopback target - and refuses one that carries no `Origin` at all. The
//! browser's headers name the public hostname, so they cannot be forwarded as
//! they are, and they cannot be dropped either. They are checked here against
//! `LAVISH_PUBLIC_BASE`, and a request that passes is handed to lavish with
//! the loopback origin it expects. One from anywhere else stops here.
//!
//! Two kinds of traffic cross this boundary. Requests are forwarded one at a
//! time and their responses streamed back. The review page's live events -
//! agent presence, agent replies, reloads, the session ending - arrive over
//! a WebSocket at `/events/<key>` (lavish 0.1.65 and later; earlier versions
//! used an SSE stream, which is a plain request). A WebSocket is not a
//! request to forward: the handshake is answered here, a second socket is
//! opened to lavish, and frames are relayed both ways until either side
//! closes. Without that relay the page never hears from its agent: it shows
//! "agent is not listening" and, after a few failed reconnects, "the server
//! is no longer running" - while feedback submitted from it is delivered.

use std::sync::OnceLock;

use axum::{
    body::Body,
    extract::{
        FromRequestParts, Request,
        ws::{self, WebSocket, WebSocketUpgrade},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{self as ts, client::IntoClientRequest};

/// Buffered request-body cap. Lavish requests are chat/feedback payloads —
/// far below this; the cap only guards against pathological bodies.
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

fn lavish_port() -> u16 {
    std::env::var("LAVISH_AXI_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4387)
}

/// Hostname (no port, lowercase) from LAVISH_PUBLIC_BASE, if configured.
fn public_host() -> Option<&'static str> {
    static HOST: OnceLock<Option<String>> = OnceLock::new();
    HOST.get_or_init(|| {
        let base = std::env::var("LAVISH_PUBLIC_BASE").ok()?;
        let parsed = url::Url::parse(base.trim()).ok()?;
        parsed.host_str().map(|host| host.to_ascii_lowercase())
    })
    .as_deref()
}

/// The origin a browser on the public hostname presents, as `Origin`
/// spells it: scheme and host from LAVISH_PUBLIC_BASE, default port elided.
fn public_origin() -> Option<&'static str> {
    static ORIGIN: OnceLock<Option<String>> = OnceLock::new();
    ORIGIN
        .get_or_init(|| {
            let base = std::env::var("LAVISH_PUBLIC_BASE").ok()?;
            Some(
                url::Url::parse(base.trim())
                    .ok()?
                    .origin()
                    .ascii_serialization(),
            )
        })
        .as_deref()
}

/// The origin lavish computes for a request that arrives from this proxy: it
/// reads the `Host` header, and both the request client and the WebSocket
/// client set that to the loopback target.
fn loopback_origin() -> String {
    format!("http://127.0.0.1:{}", lavish_port())
}

/// Whether a browser's `Origin` header names the public base.
///
/// Both sides are compared as serialized origins, so `https://h:443` and
/// `https://h` agree and `http://h` does not. `null`, garbage and anything
/// unparseable are not the public base.
fn origin_matches(origin: &str, public_origin: &str) -> bool {
    let serialized = |value: &str| {
        url::Url::parse(value)
            .ok()
            .map(|parsed| parsed.origin())
            .filter(|origin| origin.is_tuple())
            .map(|origin| origin.ascii_serialization())
    };
    match (serialized(origin), serialized(public_origin)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// The reply for a request from an origin this proxy does not front.
fn rejected(origin: &str) -> Response {
    tracing::warn!(origin, "lavish proxy: cross-origin request rejected");
    (
        http::StatusCode::FORBIDDEN,
        [(http::header::CONTENT_TYPE, "application/json")],
        r#"{"error":"cross-origin request rejected by the review proxy"}"#,
    )
        .into_response()
}

/// The reply when lavish cannot be reached, for a request or a handshake.
fn unreachable() -> Response {
    (
        http::StatusCode::BAD_GATEWAY,
        "lavish review server unreachable",
    )
        .into_response()
}

/// Client without a request timeout: lavish's UI and relay long-poll
/// (`/api/poll`) holds requests open for ~25s by design.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .build()
            .expect("failed to build lavish proxy client")
    })
}

fn request_host(req: &Request) -> Option<String> {
    let raw = req
        .headers()
        .get(http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| req.uri().host())?;
    // Strip :port; keep IPv6 literals intact.
    let host = raw
        .rsplit_once(':')
        .filter(|(head, port)| !head.contains(':') || port.chars().all(|c| c.is_ascii_digit()))
        .map(|(head, _)| head)
        .unwrap_or(raw);
    Some(host.trim_matches(['[', ']']).to_ascii_lowercase())
}

fn path_and_query(req: &Request) -> String {
    req.uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string())
}

/// Whether the request asks to become a WebSocket. The `Upgrade` header is
/// the whole signal; `WebSocketUpgrade` checks the rest of the handshake.
fn is_websocket_upgrade(req: &Request) -> bool {
    req.headers()
        .get(http::header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

/// Middleware: requests addressed to the lavish public hostname are proxied
/// to the loopback lavish server; everything else continues to the router.
pub async fn route_lavish_host(req: Request, next: Next) -> Response {
    let Some(target_host) = public_host() else {
        return next.run(req).await;
    };
    if request_host(&req).as_deref() != Some(target_host) {
        return next.run(req).await;
    }

    if is_websocket_upgrade(&req) {
        return proxy_websocket(req).await;
    }

    match proxy(req).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(?error, "lavish proxy: upstream request failed");
            unreachable()
        }
    }
}

/// Hop-by-hop headers that must not be forwarded in either direction, plus
/// forwarding metadata: the fronting proxy (caddy) adds X-Forwarded-* naming
/// the public hostname, and lavish's DNS-rebinding guard rejects requests
/// that appear addressed to a non-localhost host ("forbidden host"). Lavish
/// must see a plain loopback request.
///
/// `Origin` is stripped here and set again in `proxy` once it has been
/// checked - as the loopback origin, which is what a plain loopback request
/// carries. `Referer` stays stripped: lavish consults it only when `Origin`
/// is absent, and a browser never omits `Origin` on a submission.
fn is_hop_by_hop(name: &http::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
            | "forwarded"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-proto"
            | "origin"
            | "referer"
    )
}

async fn proxy(req: Request) -> anyhow::Result<Response> {
    let target = format!("http://127.0.0.1:{}{}", lavish_port(), path_and_query(&req));

    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes())?;
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in req.headers() {
        if !is_hop_by_hop(name) {
            headers.insert(name.clone(), value.clone());
        }
    }

    // The same-origin check, done where the origin boundary is. A browser
    // sends `Origin` on every submission and on cross-origin fetches; a
    // top-level navigation sends none, and lavish checks none on those.
    if let Some(origin) = req.headers().get(http::header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        match public_origin() {
            Some(public) if origin_matches(origin, public) => {
                headers.insert(reqwest::header::ORIGIN, loopback_origin().parse()?);
            }
            _ => return Ok(rejected(origin)),
        }
    }

    let body = axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await?;
    let upstream = client()
        .request(method, &target)
        .headers(headers)
        .body(body)
        .send()
        .await?;

    let mut response = Response::builder().status(upstream.status().as_u16());
    for (name, value) in upstream.headers() {
        if !is_hop_by_hop(name) {
            response = response.header(name, value);
        }
    }
    Ok(response.body(Body::from_stream(upstream.bytes_stream()))?)
}

/// Answers a WebSocket handshake from the browser by opening the same
/// socket to lavish and relaying frames between the two.
///
/// Lavish is connected first: if it refuses or is not running, the browser
/// sees the handshake fail (502), which its reconnect loop already handles
/// the way it handles any unreachable server. Only once lavish has accepted
/// does the browser get its 101.
async fn proxy_websocket(req: Request) -> Response {
    // A browser sends `Origin` on every WebSocket handshake, and lavish's
    // upgrade handler refuses one without it or with the wrong one - the
    // same boundary as prompts, kept here for the same reason.
    let origin = req
        .headers()
        .get(http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    match public_origin() {
        Some(public) if origin_matches(&origin, public) => {}
        _ => return rejected(&origin),
    }

    let target = format!("ws://127.0.0.1:{}{}", lavish_port(), path_and_query(&req));
    let (mut parts, _body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(upgrade) => upgrade,
        Err(rejection) => return rejection.into_response(),
    };

    let upstream = match connect_upstream(&target).await {
        Ok(upstream) => upstream,
        Err(error) => {
            tracing::warn!(?error, target, "lavish proxy: upstream websocket failed");
            return unreachable();
        }
    };

    upgrade.on_upgrade(move |browser| relay(browser, upstream))
}

type Upstream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the loopback socket lavish expects: `Host` is the loopback target
/// (the client derives it from the URL) and `Origin` the loopback origin.
async fn connect_upstream(target: &str) -> anyhow::Result<Upstream> {
    let mut request = target.into_client_request()?;
    request
        .headers_mut()
        .insert(http::header::ORIGIN, loopback_origin().parse()?);
    let (upstream, _response) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio_tungstenite::connect_async(request),
    )
    .await??;
    Ok(upstream)
}

/// Relays frames both ways until either side closes or errors; dropping the
/// halves closes whatever is still open. Pings and pongs are relayed too,
/// so lavish's heartbeat reaches the browser and the browser's answer
/// reaches lavish, which is how lavish notices a page that went away.
async fn relay(browser: WebSocket, upstream: Upstream) {
    let (mut browser_tx, mut browser_rx) = browser.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();

    let to_browser = async {
        while let Some(Ok(message)) = upstream_rx.next().await {
            let Some(message) = from_upstream(message) else {
                continue;
            };
            let closing = matches!(message, ws::Message::Close(_));
            if browser_tx.send(message).await.is_err() || closing {
                break;
            }
        }
    };
    let to_upstream = async {
        while let Some(Ok(message)) = browser_rx.next().await {
            let closing = matches!(message, ws::Message::Close(_));
            if upstream_tx.send(to_upstream(message)).await.is_err() || closing {
                break;
            }
        }
    };
    tokio::pin!(to_browser);
    tokio::pin!(to_upstream);
    tokio::select! {
        _ = &mut to_browser => {}
        _ = &mut to_upstream => {}
    }
}

// axum and tungstenite spell the same frames as two types and keep the
// conversion between them private, so it is written out here.

fn to_upstream(message: ws::Message) -> ts::Message {
    match message {
        ws::Message::Text(text) => ts::Message::Text(text.as_str().into()),
        ws::Message::Binary(bytes) => ts::Message::Binary(bytes),
        ws::Message::Ping(bytes) => ts::Message::Ping(bytes),
        ws::Message::Pong(bytes) => ts::Message::Pong(bytes),
        ws::Message::Close(frame) => {
            ts::Message::Close(frame.map(|frame| ts::protocol::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.as_str().into(),
            }))
        }
    }
}

fn from_upstream(message: ts::Message) -> Option<ws::Message> {
    Some(match message {
        ts::Message::Text(text) => ws::Message::Text(text.as_str().into()),
        ts::Message::Binary(bytes) => ws::Message::Binary(bytes),
        ts::Message::Ping(bytes) => ws::Message::Ping(bytes),
        ts::Message::Pong(bytes) => ws::Message::Pong(bytes),
        ts::Message::Close(frame) => ws::Message::Close(frame.map(|frame| ws::CloseFrame {
            code: frame.code.into(),
            reason: frame.reason.as_str().into(),
        })),
        // Raw frames are an implementation detail tungstenite says to skip.
        ts::Message::Frame(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{from_upstream, is_websocket_upgrade, origin_matches, to_upstream, ts, ws};

    const PUBLIC: &str = "https://lavish.prepcook.example.com";

    #[test]
    fn a_browser_on_the_public_hostname_is_the_public_origin() {
        assert!(origin_matches(
            "https://lavish.prepcook.example.com",
            PUBLIC
        ));
        // The default port is the same origin, however it is spelled.
        assert!(origin_matches(
            "https://lavish.prepcook.example.com:443",
            PUBLIC
        ));
        assert!(origin_matches(
            "HTTPS://LAVISH.PREPCOOK.EXAMPLE.COM",
            PUBLIC
        ));
    }

    #[test]
    fn anything_else_is_not() {
        // Scheme is part of an origin.
        assert!(!origin_matches(
            "http://lavish.prepcook.example.com",
            PUBLIC
        ));
        // So is the host, sibling or not.
        assert!(!origin_matches("https://prepcook.example.com", PUBLIC));
        assert!(!origin_matches(
            "https://lavish.prepcook.example.com.evil.example",
            PUBLIC
        ));
        // And a port that is not the default.
        assert!(!origin_matches(
            "https://lavish.prepcook.example.com:8443",
            PUBLIC
        ));
        // An opaque origin, a bare word, and nothing at all.
        assert!(!origin_matches("null", PUBLIC));
        assert!(!origin_matches("lavish.prepcook.example.com", PUBLIC));
        assert!(!origin_matches("", PUBLIC));
    }

    #[test]
    fn a_base_with_a_path_still_names_only_its_origin() {
        // LAVISH_PUBLIC_BASE may carry a trailing slash or path; the origin
        // is what the browser sends, and that has neither.
        assert!(origin_matches(
            "https://lavish.prepcook.example.com",
            "https://lavish.prepcook.example.com/"
        ));
    }

    #[test]
    fn only_a_websocket_upgrade_is_one() {
        let request = |upgrade: Option<&str>| {
            let mut builder = axum::extract::Request::builder().uri("/events/abc");
            if let Some(upgrade) = upgrade {
                builder = builder.header("upgrade", upgrade);
            }
            builder.body(axum::body::Body::empty()).unwrap()
        };
        assert!(is_websocket_upgrade(&request(Some("websocket"))));
        assert!(is_websocket_upgrade(&request(Some("WebSocket"))));
        assert!(!is_websocket_upgrade(&request(Some("h2c"))));
        assert!(!is_websocket_upgrade(&request(None)));
    }

    #[test]
    fn frames_cross_the_boundary_unchanged() {
        // The event frames are JSON text; a close carries its code and
        // reason across so the other side learns why.
        let text = from_upstream(ts::Message::Text(r#"{"type":"agent-presence"}"#.into()))
            .expect("text is relayed");
        assert_eq!(
            text,
            ws::Message::Text(r#"{"type":"agent-presence"}"#.into())
        );
        let close = to_upstream(ws::Message::Close(Some(ws::CloseFrame {
            code: 1001,
            reason: "Lavish server shutdown".into(),
        })));
        match close {
            ts::Message::Close(Some(frame)) => {
                assert_eq!(u16::from(frame.code), 1001);
                assert_eq!(frame.reason.as_str(), "Lavish server shutdown");
            }
            other => panic!("expected a close frame, got {other:?}"),
        }
        // Raw frames never reach the browser.
        assert!(
            from_upstream(ts::Message::Frame(ts::protocol::frame::Frame::pong(
                Vec::new()
            )))
            .is_none()
        );
    }
}
