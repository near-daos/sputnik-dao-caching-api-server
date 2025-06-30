use hex;
use near_jsonrpc_client::methods::query::RpcQueryRequest;
use near_jsonrpc_client::{JsonRpcClient, methods};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::AccountId;
use near_primitives::types::Finality;
use near_primitives::types::FunctionArgs;
use near_primitives::views::QueryRequest;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env as std_env;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::timeout;

static RPC_CLIENT: OnceLock<Arc<JsonRpcClient>> = OnceLock::new();

/// Returns a shared instance of the RPC client
pub fn get_rpc_client() -> Arc<JsonRpcClient> {
    RPC_CLIENT
        .get_or_init(|| {
            dotenvy::dotenv().ok();
            let rpc_url = std_env::var("NEAR_RPC_URL")
                .unwrap_or("https://archival-rpc.mainnet.fastnear.com".to_string());
            let mut client = JsonRpcClient::connect(rpc_url);
            if let Some(key) = std_env::var("NEAR_FAST_API_KEY").ok() {
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

/// Check if a DAO has a lockup account
pub async fn account_to_lockup(client: &JsonRpcClient, account_id: &str) -> Option<String> {
    if account_id.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(account_id.as_bytes());
    let byte_slice = hasher.finalize();

    let truncated_hash = &byte_slice[..20];

    let lockup_account = format!("{}.lockup.near", hex::encode(truncated_hash));

    // Check if the lockup account exists
    let request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::ViewAccount {
            account_id: lockup_account.parse().ok()?,
        },
    };

    match timeout(Duration::from_secs(5), client.call(request)).await {
        Ok(Ok(response)) => {
            if let QueryResponseKind::ViewAccount(account_view) = response.kind {
                if account_view.amount > 0 {
                    return Some(lockup_account);
                }
            }
        }
        Ok(Err(_)) => {
            // Account doesn't exist or other error
        }
        Err(_) => {
            // Timeout
        }
    }

    None
}

/// Fetch staking_pool_account_id from a lockup contract
pub async fn get_staking_pool_account_id(
    client: &JsonRpcClient,
    lockup_account: &str,
) -> Option<String> {
    let request = RpcQueryRequest {
        block_reference: Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: lockup_account.parse::<AccountId>().ok()?,
            method_name: "get_staking_pool_account_id".to_string(),
            args: FunctionArgs::from(json!({}).to_string().into_bytes()),
        },
    };

    match client.call(request).await.ok()? {
        response if matches!(response.kind, QueryResponseKind::CallResult(_)) => {
            if let QueryResponseKind::CallResult(result) = response.kind {
                serde_json::from_slice::<String>(&result.result).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}
