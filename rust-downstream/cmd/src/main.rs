use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;
use envoy_types::pb::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer;
use ipp_api_translation::{ApiTranslationPlugin, VertexOpenAiConfig};
use ipp_apikey_injection::secret_store::SecretStore;
use ipp_apikey_injection::ApiKeyInjectionPlugin;
use ipp_framework::plugin::{RequestProcessor, ResponseProcessor};
use ipp_model_provider_resolver::model_store::ModelInfoStore;
use ipp_model_provider_resolver::ModelProviderResolverPlugin;
use ipp_plugins::body_field_to_header::BodyFieldToHeaderPlugin;
use ipp_server::ext_proc_handler::ExtProcServer;
use ipp_server::metrics;
use tonic::transport::Server;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "ipp-server",
    about = "AI Gateway Payload Processor — Rust ext_proc server with upstream + downstream plugins"
)]
struct Cli {
    #[arg(long, default_value = "9004", help = "gRPC ext_proc port")]
    grpc_port: u16,

    #[arg(long, default_value = "9005", help = "Health check port")]
    health_port: u16,

    #[arg(long, default_value = "9090", help = "Prometheus metrics port")]
    metrics_port: u16,

    #[arg(long, help = "Vertex OpenAI project")]
    vertex_project: Option<String>,

    #[arg(long, help = "Vertex OpenAI location")]
    vertex_location: Option<String>,

    #[arg(long, help = "Vertex OpenAI endpoint")]
    vertex_endpoint: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    info!("Starting IPP Rust ext_proc server (upstream + downstream plugins)");

    // --- Stores ---
    let model_store = ModelInfoStore::new();
    let secret_store = SecretStore::new();

    // --- Upstream generic plugins ---
    let body_to_header = BodyFieldToHeaderPlugin::new("model", "X-Gateway-Model-Name")?;

    // --- Downstream product-specific plugins ---
    let vertex_config = match (&cli.vertex_project, &cli.vertex_location, &cli.vertex_endpoint) {
        (Some(p), Some(l), Some(e)) => Some(VertexOpenAiConfig {
            project: p.clone(),
            location: l.clone(),
            endpoint: e.clone(),
        }),
        _ => {
            warn!("Vertex OpenAI config not provided");
            None
        }
    };
    let api_translation = ApiTranslationPlugin::new(vertex_config.clone())?;
    let apikey_injection = ApiKeyInjectionPlugin::new(secret_store.clone());
    let model_resolver = ModelProviderResolverPlugin::new(model_store.clone());

    // --- Plugin chain (order matters) ---
    let request_plugins: Vec<Box<dyn RequestProcessor>> = vec![
        Box::new(body_to_header),      // 1. Extract model → header
        Box::new(model_resolver),       // 2. Resolve ExternalModel → provider (DOWNSTREAM)
        Box::new(api_translation),      // 3. Translate request format
        Box::new(apikey_injection),     // 4. Inject API key
    ];

    let api_translation_resp = ApiTranslationPlugin::new(vertex_config)?;
    let response_plugins: Vec<Box<dyn ResponseProcessor>> = vec![
        Box::new(api_translation_resp),
    ];

    // --- Start K8s reconcilers ---
    let kube_client = kube::Client::try_default().await.ok();
    if let Some(client) = kube_client {
        let ms = model_store.clone();
        let c = client.clone();
        tokio::spawn(async move {
            ipp_model_provider_resolver::reconciler::run_external_model_watcher(c, ms).await;
        });

        let ss = secret_store.clone();
        tokio::spawn(async move {
            ipp_apikey_injection::reconciler::run_secret_watcher(client, ss).await;
        });

        info!("Started ExternalModel and Secret reconcilers");
    }

    // --- Metrics ---
    let metrics_instance = metrics::Metrics::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize metrics: {}", e))?;

    let health_port = cli.health_port;
    let health_handle = tokio::spawn(async move {
        if let Err(e) = ipp_server::health::serve_health(health_port).await {
            error!(error = %e, "Health server failed");
        }
    });

    let metrics_port = cli.metrics_port;
    let metrics_clone = metrics_instance.clone();
    let metrics_handle = tokio::spawn(async move {
        if let Err(e) = metrics::serve_metrics(metrics_port, metrics_clone).await {
            error!(error = %e, "Metrics server failed");
        }
    });

    // --- Start ext_proc server ---
    let ext_proc = ExtProcServer::new(request_plugins, response_plugins, metrics_instance);
    let addr: SocketAddr = format!("0.0.0.0:{}", cli.grpc_port).parse()?;

    info!(
        grpc_port = cli.grpc_port,
        health_port = cli.health_port,
        metrics_port = cli.metrics_port,
        "Starting gRPC ext_proc server"
    );

    let shutdown = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!(error = %e, "Failed to install signal handler");
            return;
        }
        info!("Received shutdown signal, draining connections...");
    };

    tokio::select! {
        result = Server::builder()
            .add_service(ExternalProcessorServer::new(ext_proc))
            .serve_with_shutdown(addr, shutdown) => {
            result?;
        }
        _ = health_handle => {
            error!("Health server exited unexpectedly");
        }
        _ = metrics_handle => {
            error!("Metrics server exited unexpectedly");
        }
    }

    info!("Server shutdown complete");
    Ok(())
}
