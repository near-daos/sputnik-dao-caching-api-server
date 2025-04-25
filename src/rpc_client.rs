use near_jsonrpc_client::JsonRpcClient;
use std::env;
use std::sync::Arc;
use std::sync::OnceLock;

static RPC_CLIENT: OnceLock<Arc<JsonRpcClient>> = OnceLock::new();

/// Returns a shared instance of the RPC client
pub fn get_rpc_client() -> Arc<JsonRpcClient> {
    let rpc_url = env::var("NEAR_RPC_URL").unwrap_or("http://127.0.0.1:3030".to_string());
    RPC_CLIENT
        .get_or_init(|| {
            let client = JsonRpcClient::connect(rpc_url);
            Arc::new(client)
        })
        .clone()
}
