//! vintage-api: the exchange-facing HTTP verifier.
//!
//! - `GET /verify/{txid}?before=<height|incident-id>`: verdict + evidence (the CLI's `--json` report)
//! - `GET /incidents`: the registry's incidents for this network, its public key, and whether its
//!   signature verified
//! - `GET /registry/incidents.json`, `GET /registry/incidents.json.sig`: the exact signed bytes, so
//!   anyone can check the signature themselves
//! - `GET /health`: network, server, chain tip
//!
//! Read-only and stateless apart from the root cache. It keeps one connection to lightwalletd, and a
//! background task keeps roots warm from the earliest registry incident to the tip.

use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    middleware::map_response_with_state,
    response::{IntoResponse, Response},
    routing::get,
};
use clap::Parser;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Semaphore;
use vintage_core::{VintageNetwork, registry::Registry};
use vintage_verifier::{
    load_registry, lwd,
    report::{self, VerifyError},
    resolve_before, roots,
};

#[derive(Parser)]
#[command(name = "vintage-api", about = "Vintage HTTP verifier")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
    #[arg(long, default_value = "main")]
    network: VintageNetwork,
    /// lightwalletd/Zaino gRPC server host:port (TLS). Defaults to zec.rocks for the network.
    #[arg(long)]
    server: Option<String>,
    /// Directory holding incidents.json, incidents.json.sig and pubkey.hex.
    #[arg(long, default_value = "registry")]
    registry: PathBuf,
    /// Root cache (SQLite). Default: ~/.cache/vintage/cache.sqlite.
    #[arg(long)]
    cache: Option<PathBuf>,
    /// Value of Access-Control-Allow-Origin (the web front end's origin), or `*`.
    #[arg(long, default_value = "*")]
    allow_origin: String,
    /// Parallel lightwalletd streams per verification.
    #[arg(long, default_value_t = 16)]
    concurrency: usize,
    /// Verifications running at once; more wait their turn.
    #[arg(long, default_value_t = 4)]
    max_parallel: usize,
    /// Seconds between background refreshes of the roots (0 disables).
    #[arg(long, default_value_t = 30)]
    refresh_secs: u64,
}

struct AppState {
    client: lwd::Client,
    network: VintageNetwork,
    server: String,
    registry: Registry,
    registry_json: Vec<u8>,
    registry_sig: String,
    pubkey: String,
    cache: roots::Cache,
    limit: Semaphore,
    concurrency: usize,
}

type Shared = Arc<AppState>;

fn error(status: StatusCode, msg: impl ToString) -> Response {
    (status, Json(json!({ "error": msg.to_string() }))).into_response()
}

#[derive(Deserialize)]
struct VerifyQuery {
    before: Option<String>,
}

async fn verify(State(s): State<Shared>, Path(txid): Path<String>, Query(q): Query<VerifyQuery>) -> Response {
    let Some(before) = q.before else {
        return error(StatusCode::BAD_REQUEST, "missing ?before=<height|incident-id>");
    };
    let before = match resolve_before(|| Ok(s.registry.clone()), s.network, &before) {
        Ok(b) => b,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let _permit = s.limit.acquire().await.expect("semaphore open");
    match report::verify(&s.client, s.network, Some(&s.cache), &txid, before, s.concurrency).await {
        Ok(r) => Json(r).into_response(),
        Err(VerifyError::BadRequest(m)) => error(StatusCode::BAD_REQUEST, m),
        Err(VerifyError::NotFound) => error(StatusCode::NOT_FOUND, "transaction not found"),
        Err(VerifyError::Upstream(e)) => {
            eprintln!("verify: upstream error: {e:#}");
            error(StatusCode::BAD_GATEWAY, format!("lightwalletd: {e}"))
        }
    }
}

async fn incidents(State(s): State<Shared>) -> Response {
    let incidents: Vec<_> = s.registry.incidents.iter().filter(|i| i.network == s.network.name()).collect();
    Json(json!({
        "network": s.network.name(),
        "signature_verified": true,
        "public_key": s.pubkey.trim(),
        "signed_file": "/registry/incidents.json",
        "signature_file": "/registry/incidents.json.sig",
        "incidents": incidents,
    }))
    .into_response()
}

async fn get_registry_json(State(s): State<Shared>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], s.registry_json.clone()).into_response()
}

async fn get_registry_sig(State(s): State<Shared>) -> Response {
    ([(header::CONTENT_TYPE, "text/plain")], s.registry_sig.clone()).into_response()
}

async fn health(State(s): State<Shared>) -> Response {
    match lwd::tip(&s.client).await {
        Ok(tip) => Json(json!({ "status": "ok", "network": s.network.name(), "server": s.server, "chain_tip": tip }))
            .into_response(),
        Err(e) => error(StatusCode::BAD_GATEWAY, format!("lightwalletd: {e}")),
    }
}

async fn cors(State(origin): State<HeaderValue>, mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET"));
    res
}

/// Keep roots computed from the earliest incident to the tip, so requests only read the cache.
async fn refresher(s: Shared, every: Duration) {
    let Some(from) = s
        .registry
        .incidents
        .iter()
        .filter(|i| i.network == s.network.name())
        .map(|i| i.first_inflow_height)
        .min()
    else {
        return;
    };
    loop {
        let t = std::time::Instant::now();
        let run = async {
            let (tip, hash) = lwd::tip_id(&s.client).await?;
            let r = roots::roots_for(&s.client, Some(&s.cache), from, tip, &hash, s.concurrency).await?;
            anyhow::Ok((tip, r.stats))
        };
        match run.await {
            Ok((tip, st)) if st.blocks_downloaded > 0 => eprintln!(
                "refresh: roots {from}..={tip}: {} blocks computed in {:.1}s",
                st.blocks_downloaded,
                t.elapsed().as_secs_f64()
            ),
            Ok(_) => {}
            Err(e) => eprintln!("refresh: {e:#}"),
        }
        tokio::time::sleep(every).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let a = Args::parse();
    let server = a.server.unwrap_or_else(|| a.network.default_server().to_string());
    let registry = load_registry(&a.registry)?;
    let registry_json = std::fs::read(a.registry.join("incidents.json"))?;
    let registry_sig = std::fs::read_to_string(a.registry.join("incidents.json.sig"))?;
    let pubkey = std::fs::read_to_string(a.registry.join("pubkey.hex"))?;
    let cache_path = a.cache.or_else(roots::default_cache_path).context("no cache path; pass --cache")?;
    let cache = roots::Cache::open(&cache_path, a.network)?;
    let client = lwd::connect(&server).await?;
    let origin = HeaderValue::from_str(&a.allow_origin).context("--allow-origin")?;

    let state = Arc::new(AppState {
        client,
        network: a.network,
        server,
        registry,
        registry_json,
        registry_sig,
        pubkey,
        cache,
        limit: Semaphore::new(a.max_parallel),
        concurrency: a.concurrency,
    });
    if a.refresh_secs > 0 {
        tokio::spawn(refresher(state.clone(), Duration::from_secs(a.refresh_secs)));
    }

    let app = Router::new()
        .route("/verify/{txid}", get(verify))
        .route("/incidents", get(incidents))
        .route("/registry/incidents.json", get(get_registry_json))
        .route("/registry/incidents.json.sig", get(get_registry_sig))
        .route("/health", get(health))
        .layer(map_response_with_state(origin, cors))
        .with_state(state.clone());

    eprintln!(
        "vintage-api: {} on {}, lightwalletd {}, cache {}, {} incident(s)",
        state.network.name(),
        a.listen,
        state.server,
        cache_path.display(),
        state.registry.incidents.iter().filter(|i| i.network == state.network.name()).count()
    );
    let listener = tokio::net::TcpListener::bind(a.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
