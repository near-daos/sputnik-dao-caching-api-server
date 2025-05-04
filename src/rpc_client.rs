use near_jsonrpc_client::JsonRpcClient;
use std::env;
use std::sync::Arc;
use std::sync::OnceLock;

static RPC_CLIENT: OnceLock<Arc<JsonRpcClient>> = OnceLock::new();

/// Returns a shared instance of the RPC client
pub fn get_rpc_client() -> Arc<JsonRpcClient> {
    RPC_CLIENT
        .get_or_init(|| {
            let rpc_url = env::var("NEAR_RPC_URL").unwrap_or("http://127.0.0.1:3030".to_string());
            let mut client = JsonRpcClient::connect(rpc_url);
            if let Some(key) = env::var("NEAR_FAST_API_KEY").ok() {
                let headers = client.headers_mut();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&key).unwrap(),
                );
            }
            Arc::new(client)
        })
        .clone()
}
