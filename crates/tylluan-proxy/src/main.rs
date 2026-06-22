use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::net::TcpStream;
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use hyper::server::conn::http1;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty};
use bytes::Bytes;
use tracing::{info, error, warn, debug};
use tracing_subscriber::EnvFilter;

struct ProxyState {
    active_port: RwLock<Option<u16>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    info!("🚀 Starting TylluanNexus Zero-Downtime Proxy Gateway...");

    let state = Arc::new(ProxyState {
        active_port: RwLock::new(None),
    });

    // Spawn active_port.json watcher
    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut last_port = None;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let path = std::path::Path::new("data/active_port.json");
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(port) = val.get("port").and_then(|p| p.as_u64()).map(|p| p as u16) {
                            if Some(port) != last_port {
                                info!("🔄 Active port updated in JSON: {} -> {}", last_port.unwrap_or(0), port);
                                last_port = Some(port);
                                *state_clone.active_port.write().await = Some(port);
                            }
                        }
                    }
                }
            }
        }
    });

    // Listen on localhost:3030
    let addr = SocketAddr::from(([127, 0, 0, 1], 3030));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("🎧 Proxy Gateway listening on http://{}", addr);

    let state_clone = state.clone();
    loop {
        let (stream, client_addr) = tokio::select! {
            res = listener.accept() => match res {
                Ok(val) => val,
                Err(e) => {
                    warn!("Failed to accept incoming connection: {}", e);
                    continue;
                }
            },
            _ = tokio::signal::ctrl_c() => {
                info!("🛑 Ctrl+C received — initiating graceful shutdown...");
                let port = *state_clone.active_port.read().await;
                if let Some(p) = port {
                    match send_shutdown_signal(p).await {
                        Ok(_) => info!("✅ Kernel shutdown signal sent to port {}", p),
                        Err(e) => warn!("⚠️ Could not reach kernel for shutdown: {}", e),
                    }
                }
                info!("👋 Proxy exiting.");
                std::process::exit(0);
            }
        };

        let state = state_clone.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = hyper::service::service_fn(move |req: Request<Incoming>| {
                let state = state.clone();
                async move {
                    match handle_request(req, state, client_addr).await {
                        Ok(res) => Ok::<_, Infallible>(res),
                        Err(e) => {
                            error!("Error handling request: {}", e);
                            let mut res = Response::new(empty_body());
                            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Ok::<_, Infallible>(res)
                        }
                    }
                }
            });

            if let Err(err) = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, service)
                .await
            {
                debug!("Error serving connection from {}: {}", client_addr, err);
            }
        });
    }
}

async fn connect_to_backend(port: u16) -> Result<TcpStream, std::io::Error> {
    let addr = format!("127.0.0.1:{}", port);
    let mut last_err = None;
    for attempt in 1..=5 {
        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                if attempt > 1 {
                    info!("🔌 Connected to backend on port {} after {} attempts", port, attempt);
                }
                return Ok(stream);
            }
            Err(e) => {
                warn!("⚠️ Attempt {} to connect to backend on port {} failed: {}", attempt, port, e);
                last_err = Some(e);
                if attempt < 5 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Failed to connect to backend after 5 attempts",
        )
    }))
}

fn empty_body() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

async fn send_shutdown_signal(port: u16) -> anyhow::Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let stream = TcpStream::connect(&addr).await?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(async move { let _ = conn.await; });
    let req = Request::builder()
        .method("POST")
        .uri(format!("http://{}/api/v1/admin/shutdown", addr))
        .body(http_body_util::Empty::<Bytes>::new().map_err(|never| match never {}).boxed())?;
    let _ = sender.send_request(req).await?;
    Ok(())
}


async fn handle_request(
    mut req: Request<Incoming>,
    state: Arc<ProxyState>,
    client_addr: SocketAddr,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, anyhow::Error> {
    // 1. Get active port with retry/wait if kernel is reloading
    let mut port_opt = *state.active_port.read().await;
    if port_opt.is_none() {
        // Wait up to 3 seconds for a port to become active
        for _ in 0..15 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            port_opt = *state.active_port.read().await;
            if port_opt.is_some() {
                break;
            }
        }
    }

    let port = match port_opt {
        Some(p) => p,
        None => {
            warn!("No active backend port available for request: {}", req.uri().path());
            let mut res = Response::new(empty_body());
            *res.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            return Ok(res);
        }
    };

    let backend_host = format!("127.0.0.1:{}", port);
    
    // Check if the request is a WebSocket upgrade
    let is_websocket = req.headers()
        .get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_websocket {
        info!("🔌 Handling WebSocket upgrade to backend on port {}", port);
        
        // Setup client upgrade future BEFORE sending the request
        let client_upgraded = hyper::upgrade::on(&mut req);

        let (mut parts, body) = req.into_parts();
        
        // Rewrite URI to backend
        let mut uri_parts = parts.uri.into_parts();
        uri_parts.authority = Some(backend_host.parse()?);
        uri_parts.scheme = Some(hyper::http::uri::Scheme::HTTP);
        let backend_uri = hyper::Uri::from_parts(uri_parts)?;
        parts.uri = backend_uri;

        // Set forwarding headers
        parts.headers.insert(
            hyper::header::FORWARDED,
            format!("for={}", client_addr.ip()).parse()?,
        );
        parts.headers.insert(
            "X-Forwarded-For",
            client_addr.ip().to_string().parse()?,
        );

        let req = Request::from_parts(parts, body);

        // Connect to backend
        let backend_stream = match connect_to_backend(port).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to connect to backend for WebSocket upgrade: {}", e);
                let mut res = Response::new(empty_body());
                *res.status_mut() = StatusCode::BAD_GATEWAY;
                return Ok(res);
            }
        };

        // Send handshake request to backend
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(backend_stream)).await?;
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!("Connection failed: {:?}", err);
            }
        });

        let mut backend_res = sender.send_request(req.map(|b| b.boxed())).await?;

        if backend_res.status() == StatusCode::SWITCHING_PROTOCOLS {
            // Setup backend upgrade future
            let backend_upgraded = hyper::upgrade::on(&mut backend_res);

            // Spawn the bi-directional copy task
            tokio::spawn(async move {
                match tokio::join!(client_upgraded, backend_upgraded) {
                    (Ok(client), Ok(backend)) => {
                        let (mut client_read, mut client_write) = tokio::io::split(TokioIo::new(client));
                        let (mut backend_read, mut backend_write) = tokio::io::split(TokioIo::new(backend));
                        
                        let client_to_backend = tokio::io::copy(&mut client_read, &mut backend_write);
                        let backend_to_client = tokio::io::copy(&mut backend_read, &mut client_write);
                        
                        if let Err(e) = tokio::try_join!(client_to_backend, backend_to_client) {
                            debug!("WebSocket tunnel closed: {}", e);
                        }
                    }
                    (Err(e), _) => error!("Client WebSocket upgrade failed: {}", e),
                    (_, Err(e)) => error!("Backend WebSocket upgrade failed: {}", e),
                }
            });
        }

        let (parts, body) = backend_res.into_parts();
        return Ok(Response::from_parts(parts, body.boxed()));
    }

    // 2. Regular HTTP / SSE Request forwarding
    let (mut parts, body) = req.into_parts();
    
    // Set appropriate headers for proxying
    parts.headers.insert(
        hyper::header::FORWARDED,
        format!("for={}", client_addr.ip()).parse()?,
    );
    parts.headers.insert(
        "X-Forwarded-For",
        client_addr.ip().to_string().parse()?,
    );

    // Rewrite URI
    let mut uri_parts = parts.uri.into_parts();
    uri_parts.authority = Some(backend_host.parse()?);
    uri_parts.scheme = Some(hyper::http::uri::Scheme::HTTP);
    let backend_uri = hyper::Uri::from_parts(uri_parts)?;
    parts.uri = backend_uri;

    let req = Request::from_parts(parts, body);

    // Connect to backend
    let backend_stream = match connect_to_backend(port).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to connect to backend http: {}", e);
            let mut res = Response::new(empty_body());
            *res.status_mut() = StatusCode::BAD_GATEWAY;
            return Ok(res);
        }
    };

    // Forward HTTP/1.1
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(backend_stream)).await?;
    tokio::spawn(async move {
        if let Err(err) = conn.await {
            error!("Connection failed: {:?}", err);
        }
    });

    let backend_res = sender.send_request(req.map(|b| b.boxed())).await?;
    let (parts, body) = backend_res.into_parts();

    Ok(Response::from_parts(parts, body.boxed()))
}
