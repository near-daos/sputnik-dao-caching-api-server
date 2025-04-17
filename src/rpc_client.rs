use near_jsonrpc_client::JsonRpcClient;
use std::sync::Arc;
use std::sync::OnceLock;

static RPC_CLIENT: OnceLock<Arc<JsonRpcClient>> = OnceLock::new();

const DEFAULT_TESTNET_RPC_URL: &str = "http://127.0.0.1:3030";

/// Returns a shared instance of the RPC client
pub fn get_rpc_client() -> Arc<JsonRpcClient> {
    RPC_CLIENT
        .get_or_init(|| {
            let client = JsonRpcClient::connect(DEFAULT_TESTNET_RPC_URL);
            Arc::new(client)
        })
        .clone()
}

/// Creates a new RPC client with a custom URL
pub fn create_rpc_client(url: &str) -> JsonRpcClient {
    JsonRpcClient::connect(url)
}
