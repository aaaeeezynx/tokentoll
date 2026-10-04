#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("GW-PANIC: {info}");
    }));
    let dbdir = dirs::data_dir()
        .map(|d| d.join("com.tokencounter.gateway"))
        .unwrap();
    let db_path = dbdir.join("app.db");
    if !db_path.exists() {
        eprintln!("no DB at {}", db_path.display());
        return;
    }
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(15722);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    eprintln!("Token Toll proxy listening on 127.0.0.1:{port}");
    eprintln!("DB: {}", db_path.display());
    match tokentoll_lib::proxy::serve(db_path, listener).await {
        Ok(()) => eprintln!("proxy exited normally"),
        Err(e) => eprintln!("proxy error: {e}"),
    }
}
