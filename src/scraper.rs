use anyhow::Result;
use near_jsonrpc_client::{JsonRpcClient, methods};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::hash::CryptoHash;
use near_primitives::types::AccountId;

use near_primitives::views::{ActionView, ReceiptEnumView};
use near_primitives::{types::FunctionArgs, views::QueryRequest};
use near_sdk::BlockHeight;
use near_sdk::json_types::U64;
use rocket::futures::future::try_join_all;
use rocket::serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

use borsh::BorshDeserialize;

use rocket::form::FromFormField;

#[derive(Serialize, Clone, Debug)]
pub struct TxMetadata {
    pub signer_id: AccountId,
    pub predecessor_id: AccountId,
    pub reciept_hash: CryptoHash,
    pub block_height: BlockHeight,
    pub timestamp: u64,
}

const PROPOSAL_LIMIT: u64 = 500;
const LOG_LIMIT: usize = 20;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum Vote {
    Approve,
    Reject,
    Remove,
}

#[derive(FromFormField, Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    InProgress,
    Approved,
    Rejected,
    Removed,
    Expired,
    Moved,
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum Action {
    AddProposal,
    RemoveProposal,
    VoteApprove,
    VoteReject,
    VoteRemove,
    Finalize,
    MoveToHub,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ProposalLog {
    pub block_height: U64,
}

#[derive(BorshDeserialize, Clone, Debug)]
pub enum StateVersion {
    V1,
    V2,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: String,
    pub description: String,
    pub kind: Value,
    pub status: ProposalStatus,
    pub vote_counts: HashMap<String, [String; 3]>,
    pub votes: HashMap<String, Vote>,
    pub submission_time: U64,
    pub last_actions_log: Option<Vec<ProposalLog>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Policy {
    pub roles: Vec<Value>,
    pub default_vote_policy: Value,
    pub proposal_bond: String, // u128
    pub proposal_period: U64,
    pub bounty_bond: String, //u128
    pub bounty_forgiveness_period: U64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionLog {
    pub account_id: AccountId,
    pub proposal_id: U64,
    pub action: Action,
    pub block_height: U64,
}

pub async fn fetch_proposals(
    client: &JsonRpcClient,
    dao_id: &AccountId,
) -> anyhow::Result<Vec<Proposal>> {
    // Get the last proposal ID
    let last_id_request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: dao_id.clone(),
            method_name: "get_last_proposal_id".to_string(),
            args: FunctionArgs::from(vec![]),
        },
    };

    let last_id_response = client.call(last_id_request).await?;
    let last_id = if let QueryResponseKind::CallResult(result) = last_id_response.kind {
        serde_json::from_slice::<u64>(&result.result)?
    } else {
        return Err(anyhow::anyhow!("Failed to get last proposal ID"));
    };

    let mut all_proposals = Vec::new();
    let mut current_index = 0;

    // Fetch proposals in batches
    while current_index < last_id {
        let limit = std::cmp::min(PROPOSAL_LIMIT, last_id - current_index);

        let query_args = FunctionArgs::from(
            json!({
                "from_index": current_index,
                "limit": limit
            })
            .to_string()
            .into_bytes(),
        );
        let request = methods::query::RpcQueryRequest {
            block_reference: near_primitives::types::Finality::Final.into(),
            request: QueryRequest::CallFunction {
                account_id: dao_id.clone(),
                method_name: "get_proposals".to_string(),
                args: query_args,
            },
        };

        let response = client.call(request).await?;
        if let QueryResponseKind::CallResult(result) = response.kind {
            let proposals_batch: Vec<Proposal> = serde_json::from_slice(&result.result)?;
            all_proposals.extend(proposals_batch);
            current_index += limit;
        } else {
            return Err(anyhow::anyhow!(
                "Unexpected response kind while fetching proposals batch starting at index {}",
                current_index
            ));
        }
        // Add a small delay to avoid hitting rate limits
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    Ok(all_proposals)
}

pub async fn fetch_proposal(
    client: &JsonRpcClient,
    dao_id: &AccountId,
    proposal_id: u64,
) -> anyhow::Result<Proposal> {
    let query_args = FunctionArgs::from(
        json!({
            "id": proposal_id,
        })
        .to_string()
        .into_bytes(),
    );
    let request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: dao_id.clone(),
            method_name: "get_proposal".to_string(),
            args: query_args,
        },
    };
    let response = client.call(request).await?;
    if let QueryResponseKind::CallResult(result) = response.kind {
        let proposal: Proposal = serde_json::from_slice(&result.result)?;
        Ok(proposal)
    } else {
        Err(anyhow::anyhow!("Failed to get proposal"))
    }
}

pub async fn fetch_proposal_at_block(
    client: &JsonRpcClient,
    dao_id: &AccountId,
    proposal_id: u64,
    block_height: u64,
) -> anyhow::Result<Proposal> {
    let query_args = FunctionArgs::from(
        json!({
            "id": proposal_id,
        })
        .to_string()
        .into_bytes(),
    );
    let request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::BlockReference::BlockId(
            near_primitives::types::BlockId::Height(block_height),
        ),
        request: QueryRequest::CallFunction {
            account_id: dao_id.clone(),
            method_name: "get_proposal".to_string(),
            args: query_args,
        },
    };
    let response = client.call(request).await?;
    if let QueryResponseKind::CallResult(result) = response.kind {
        let proposal: Proposal = serde_json::from_slice(&result.result)?;
        Ok(proposal)
    } else {
        Err(anyhow::anyhow!(
            "Failed to get proposal at block {}",
            block_height
        ))
    }
}

pub async fn fetch_proposal_log_txs(
    client: &JsonRpcClient,
    dao_id: &AccountId,
    proposal_id: u64,
    block_height_limit: u64,
) -> anyhow::Result<Vec<TxMetadata>> {
    let proposal = fetch_proposal(client, dao_id, proposal_id).await?;
    if proposal.last_actions_log.is_none() {
        return Ok(Vec::new());
    }

    let mut earliest_log = proposal.last_actions_log.unwrap();
    let mut complete_log = Vec::new();

    while earliest_log.len() == LOG_LIMIT {
        let earliest_block_height = earliest_log.first().unwrap().block_height.0;
        // When the blocks are too deep - break
        if earliest_block_height < block_height_limit {
            break;
        }
        // Extends in a wrong order
        complete_log.extend(earliest_log);
        let earlier_block_height = earliest_block_height - 1;
        earliest_log = fetch_proposal_at_block(client, dao_id, proposal_id, earlier_block_height)
            .await?
            .last_actions_log
            .unwrap();
    }
    let earliest_log: Vec<ProposalLog> = earliest_log
        .iter()
        .filter(|log| log.block_height.0 > block_height_limit)
        .cloned()
        .collect();
    complete_log.extend(earliest_log);
    // Sort is required because of extend in a wrong order
    complete_log.sort_by_key(|l| l.block_height.0);
    complete_log.dedup();

    let futures = complete_log
        .iter()
        .map(|l| l.block_height.0)
        .map(|block_number| fetch_proposal_txs_in_block(client, dao_id, proposal_id, block_number));
    let res = try_join_all(futures).await?.into_iter().flatten().collect();

    Ok(res)
}

pub async fn fetch_policy(client: &JsonRpcClient, dao_id: &AccountId) -> anyhow::Result<Policy> {
    let request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: dao_id.clone(),
            method_name: "get_policy".to_string(),
            args: FunctionArgs::from(vec![]),
        },
    };

    let response = client.call(request).await?;

    if let QueryResponseKind::CallResult(result) = response.kind {
        let policy: Policy = serde_json::from_slice(&result.result)?;
        Ok(policy)
    } else {
        Err(anyhow::anyhow!("Failed to get policy"))
    }
}

pub async fn fetch_contract_version(
    client: &JsonRpcClient,
    dao_id: &AccountId,
) -> anyhow::Result<StateVersion> {
    let request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::ViewState {
            account_id: dao_id.clone(),
            prefix: "STATEVERSION".as_bytes().to_vec().into(),
            include_proof: false,
        },
    };

    let response = client.call(request).await;
    match response {
        Ok(result) => {
            if let QueryResponseKind::ViewState(call_result) = result.kind {
                if let Some(value) = call_result.values.get(0) {
                    let version = StateVersion::try_from_slice(&value.value)?;
                    Ok(version)
                } else {
                    Ok(StateVersion::V1)
                }
            } else {
                Err(anyhow::anyhow!("Failed to get contract version"))
            }
        }
        Err(_) => Ok(StateVersion::V1), // If the call fails, version is V1
    }
}

pub async fn fetch_actions_log(
    client: &JsonRpcClient,
    dao_id: &AccountId,
) -> Option<Vec<ActionLog>> {
    let request = methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: dao_id.clone(),
            method_name: "get_actions_log".to_string(),
            args: FunctionArgs::from(vec![]),
        },
    };

    match client.call(request).await {
        Ok(response) => {
            if let QueryResponseKind::CallResult(result) = response.kind {
                match serde_json::from_slice::<Vec<ActionLog>>(&result.result) {
                    Ok(actions_log) => Some(actions_log),
                    Err(_) => None,
                }
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

pub async fn fetch_proposal_txs_in_block(
    client: &JsonRpcClient,
    dao_id: &AccountId,
    proposal_id: u64,
    block_height: u64,
) -> Result<Vec<TxMetadata>> {
    let block_request = methods::block::RpcBlockRequest {
        block_reference: near_primitives::types::BlockReference::BlockId(
            near_primitives::types::BlockId::Height(block_height),
        ),
    };
    let block_response = client.call(block_request).await?;

    let chunks_views = block_response.chunks;
    let timestamp = block_response.header.timestamp;

    let chunk_futures = chunks_views.iter().map(|chunk_header| {
        let chunk_request = methods::chunk::RpcChunkRequest {
            chunk_reference: methods::chunk::ChunkReference::ChunkHash {
                chunk_id: chunk_header.chunk_hash,
            },
        };
        client.call(chunk_request)
    });
    let chunk_results = try_join_all(chunk_futures).await?;

    let mut proposal_txs = Vec::new();
    for chunk in chunk_results {
        for rc in &chunk.receipts {
            if &rc.receiver_id == dao_id {
                if let ReceiptEnumView::Action {
                    signer_id, actions, ..
                } = rc.receipt.clone()
                {
                    for action in actions {
                        if let ActionView::FunctionCall {
                            method_name, args, ..
                        } = action
                        {
                            match method_name.as_str() {
                                "act_proposal" => {
                                    let args: Value = serde_json::from_slice(&args)
                                        .expect("Couldn't deserialize args.");
                                    let id = args
                                        .get("id")
                                        .expect("No id found at proposal.")
                                        .as_u64()
                                        .unwrap();
                                    if proposal_id == id {
                                        proposal_txs.push(TxMetadata {
                                            signer_id: signer_id.clone(),
                                            predecessor_id: rc.predecessor_id.clone(),
                                            reciept_hash: rc.receipt_id,
                                            block_height,
                                            timestamp,
                                        })
                                    }
                                }
                                // There will be mismatch if two proposals are created in the same block.
                                "add_proposal" => proposal_txs.push(TxMetadata {
                                    signer_id: signer_id.clone(),
                                    predecessor_id: rc.predecessor_id.clone(),
                                    reciept_hash: rc.receipt_id,
                                    block_height,
                                    timestamp,
                                }),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(proposal_txs)
}
