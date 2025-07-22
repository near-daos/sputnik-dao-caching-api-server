use near_jsonrpc_client::JsonRpcClient;
use near_jsonrpc_client::methods::query::RpcQueryRequest;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::AccountId;
use near_primitives::types::FunctionArgs;
use rocket::local::blocking::Client;
use serde_json::Value;
use sputnik_indexer;

// Test data structures
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Proposal {
    id: u64,
    proposer: String,
    description: String,
    kind: Value,
    status: String,
    vote_counts: Value,
    votes: Value,
    submission_time: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IndexerResponse {
    proposals: Vec<Proposal>,
    total: u64,
    page: u64,
    page_size: u64,
}

// Helper function to check for transfer proposals (payments category)
fn check_for_transfer_proposals(item: &Proposal) -> bool {
    // Check for proposal_action in description
    if let Ok(parsed) = serde_json::from_str::<Value>(&item.description) {
        if let Some(proposal_action) = parsed.get("proposal_action") {
            if proposal_action.as_str() == Some("transfer") {
                return true;
            }
        }
    }

    // Check for Transfer kind
    if item.kind.get("Transfer").is_some() {
        return true;
    }

    // Check for ft_withdraw or ft_transfer method calls
    if let Some(function_call) = item.kind.get("FunctionCall") {
        if let Some(actions) = function_call.get("actions") {
            if let Some(actions_array) = actions.as_array() {
                for action in actions_array {
                    if let Some(method_name) = action.get("method_name") {
                        let method = method_name.as_str().unwrap_or("");
                        if method == "ft_withdraw" || method == "ft_transfer" {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

// Helper function to check for asset exchange proposals
fn check_for_asset_exchange_proposals(item: &Proposal) -> bool {
    // Try to parse as JSON for proposal_action
    if let Ok(parsed) = serde_json::from_str::<Value>(&item.description) {
        if let Some(proposal_action) = parsed.get("proposal_action") {
            if proposal_action.as_str() == Some("asset-exchange") {
                return true;
            }
        }
    }

    // Check for "Proposal Action: asset-exchange" in description
    if item.description.contains("Proposal Action: asset-exchange") {
        return true;
    }

    // Markdown fallback - check for "* Proposal Action: asset-exchange"
    // Handle both <br> and \n line separators
    let lines: Vec<&str> = item.description.split("<br>").collect();
    for line in lines {
        // Also split by newlines within each <br> section
        let sublines: Vec<&str> = line.split('\n').collect();
        for subline in sublines {
            if subline.starts_with("* ") {
                let rest = &subline[2..];
                if let Some(colon_index) = rest.find(':') {
                    let key = rest[..colon_index].trim().to_lowercase();
                    let value = rest[colon_index + 1..].trim();
                    if key == "proposal action" && value == "asset-exchange" {
                        return true;
                    }
                }
            }
        }
    }

    false
}

// Helper function to check for stake delegation proposals
fn check_for_stake_delegation_proposals(item: &Proposal) -> bool {
    // Try to parse as JSON for proposal_action
    if let Ok(parsed) = serde_json::from_str::<Value>(&item.description) {
        if let Some(proposal_action) = parsed.get("proposal_action") {
            if let Some(action) = proposal_action.as_str() {
                if action == "stake" || action == "unstake" || action == "withdraw" {
                    return true;
                }
            }
        }

        // Check for isStakeRequest field
        let is_stake_request = parsed
            .get("isStakeRequest")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_stake_request {
            return true;
        }
    }

    // Markdown fallback - check for "* Proposal Action: stake/unstake/withdraw"
    // Handle both <br> and \n line separators
    let lines: Vec<&str> = item.description.split("<br>").collect();
    for line in lines {
        // Also split by newlines within each <br> section
        let sublines: Vec<&str> = line.split('\n').collect();
        for subline in sublines {
            if subline.starts_with("* ") {
                let rest = &subline[2..];
                if let Some(colon_index) = rest.find(':') {
                    let key = rest[..colon_index].trim().to_lowercase();
                    let value = rest[colon_index + 1..].trim();
                    if key == "proposal action"
                        && (value == "stake" || value == "unstake" || value == "withdraw")
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

// Helper function to check for lockup proposals
fn check_for_lockup_proposals(item: &Proposal) -> bool {
    if let Some(function_call) = item.kind.get("FunctionCall") {
        if let Some(receiver_id) = function_call.get("receiver_id") {
            if receiver_id.as_str() == Some("lockup.near") {
                return true;
            }
        }
    }

    false
}

// Helper function to filter proposals by category
fn filter_proposals_by_category(proposals: &[Proposal], category: &str) -> Vec<Proposal> {
    proposals
        .iter()
        .filter(|proposal| {
            match category {
                "payments" => check_for_transfer_proposals(proposal),
                "asset-exchange" => check_for_asset_exchange_proposals(proposal),
                "stake-delegation" => check_for_stake_delegation_proposals(proposal),
                "lockup" => check_for_lockup_proposals(proposal),
                _ => true, // No filtering for unknown categories
            }
        })
        .cloned()
        .collect()
}

// Helper function to fetch from indexer
fn fetch_from_indexer(
    dao_id: &str,
    category: Option<&str>,
) -> Result<IndexerResponse, Box<dyn std::error::Error>> {
    let client = Client::tracked(sputnik_indexer::rocket()).expect("valid rocket instance");

    let mut url = format!(
        "/proposals/{}?sort_by=CreationTime&sort_direction=asc",
        dao_id
    );
    if let Some(cat) = category {
        url.push_str(&format!("&category={}", cat));
    }

    let response = client.get(&url).dispatch();
    let response_text = response.into_string().expect("response body");
    let indexer_response: IndexerResponse = serde_json::from_str(&response_text)?;
    Ok(indexer_response)
}

// Helper function to fetch from RPC
async fn fetch_from_rpc(
    client: &JsonRpcClient,
    dao_id: &str,
    from_index: u64,
    limit: u64,
) -> Result<Vec<Proposal>, Box<dyn std::error::Error>> {
    let args = serde_json::json!({
        "from_index": from_index,
        "limit": limit
    });
    let request = RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: near_primitives::views::QueryRequest::CallFunction {
            account_id: dao_id.parse::<AccountId>()?,
            method_name: "get_proposals".to_string(),
            args: FunctionArgs::from(serde_json::to_vec(&args)?),
        },
    };
    let response = client.call(request).await?;
    if let QueryResponseKind::CallResult(result) = response.kind {
        let proposals: Vec<Proposal> = serde_json::from_slice(&result.result)?;
        Ok(proposals)
    } else {
        Err("Unexpected response kind".into())
    }
}

#[test]
fn test_indexer_rpc_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let dao_id = "testing-astradao.sputnik-dao.near";

    // Initialize RPC client
    let client = JsonRpcClient::connect("https://archival-rpc.mainnet.fastnear.com");

    // Test each category
    let categories = ["payments", "stake-delegation", "lockup", "asset-exchange"];

    for category in &categories {
        // Fetch from indexer with category filter
        let indexer_response = fetch_from_indexer(dao_id, Some(category))?;

        // Fetch from RPC (all proposals, we'll filter them) - use tokio::runtime
        let rt = tokio::runtime::Runtime::new()?;
        let rpc_proposals = rt.block_on(fetch_from_rpc(&client, dao_id, 0, 500))?;

        // Filter RPC proposals by category
        let filtered_rpc_proposals = filter_proposals_by_category(&rpc_proposals, category);

        // Check if there's a mismatch
        if indexer_response.proposals.len() != filtered_rpc_proposals.len() {
            // Find proposals that are in RPC but not in indexer
            let indexer_ids: std::collections::HashSet<u64> =
                indexer_response.proposals.iter().map(|p| p.id).collect();
            let rpc_ids: std::collections::HashSet<u64> =
                filtered_rpc_proposals.iter().map(|p| p.id).collect();

            let missing_in_indexer: Vec<u64> = rpc_ids.difference(&indexer_ids).cloned().collect();
            let missing_in_rpc: Vec<u64> = indexer_ids.difference(&rpc_ids).cloned().collect();

            if !missing_in_indexer.is_empty() {
                println!(
                    "❌ Category {}: Proposals in RPC but missing from indexer: {:?}",
                    category, missing_in_indexer
                );
            }
            if !missing_in_rpc.is_empty() {
                println!(
                    "❌ Category {}: Proposals in indexer but missing from RPC: {:?}",
                    category, missing_in_rpc
                );
            }

            // Fail the test on mismatch
            assert_eq!(
                indexer_response.proposals.len(),
                filtered_rpc_proposals.len(),
                "Number of proposals should match between indexer and RPC for category {}",
                category
            );
        }
    }

    Ok(())
}
