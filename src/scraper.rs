use anyhow::Result;
use near_jsonrpc_client::{JsonRpcClient, methods};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::hash::CryptoHash;
use near_primitives::types::AccountId;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose};
use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{TimeZone, Utc};
use futures::FutureExt;
use futures::future::BoxFuture;

use crate::cache::{FtMetadataCache, get_ft_metadata_cache};
use near_jsonrpc_client::methods::query::RpcQueryRequest;
use near_primitives::views::{ActionView, ReceiptEnumView};
use near_primitives::{types::FunctionArgs, views::QueryRequest};
use near_sdk::BlockHeight;
use near_sdk::json_types::{U64, U128};
use rocket::form::FromFormField;
use rocket::futures::future::try_join_all;
use rocket::serde::{Deserialize, Serialize};

use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::from_slice;
use serde_json::json;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, BorshSerialize, BorshDeserialize, Clone, Debug)]
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

#[derive(
    FromFormField,
    Debug,
    Deserialize,
    Serialize,
    BorshSerialize,
    BorshDeserialize,
    Clone,
    PartialEq,
    Eq,
)]
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
#[serde(untagged)]
pub enum CountsVersions {
    // In actual contract u128 is used
    V1(u64),
    V2(U128),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: String,
    pub description: String,
    pub kind: Value,
    pub status: ProposalStatus,
    pub vote_counts: HashMap<String, [CountsVersions; 3]>,
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct FtMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<String>,
}

impl FtMetadata {
    pub fn near() -> Self {
        FtMetadata {
            name: "Near".to_string(),
            symbol: "NEAR".to_string(),
            decimals: 24,
            icon: None,
            reference: None,
            reference_hash: None,
        }
    }

    pub fn empty() -> Self {
        FtMetadata {
            name: "".to_string(),
            symbol: "".to_string(),
            decimals: 0,
            icon: None,
            reference: None,
            reference_hash: None,
        }
    }
}

pub struct TransferProposalFormatter;
pub struct LockupProposalFormatter;
pub struct StakeDelegationProposalFormatter;
pub struct AssetExchangeProposalFormatter;
pub struct StakeDelegationroposalFormatter;
pub struct DefaultFormatter;

#[derive(Deserialize, Debug)]
struct VestingSchedule {
    cliff_timestamp: Option<String>,
    end_timestamp: Option<String>,
    start_timestamp: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct VestingScheduleWrapper {
    #[serde(rename = "VestingSchedule")]
    vesting_schedule: Option<VestingSchedule>,
}

#[derive(Deserialize, Debug)]
struct LockupArgs {
    owner_account_id: Option<String>,
    lockup_timestamp: Option<String>,
    release_duration: Option<String>,
    vesting_schedule: Option<VestingScheduleWrapper>,
    whitelist_account_id: Option<String>,
}

pub trait ProposalCsvFormatterSync: Send + Sync {
    fn headers(&self) -> Vec<&'static str>;
    fn format(&self, proposal: &Proposal, policy: &Policy) -> Vec<String>;
}

pub trait ProposalCsvFormatterAsync: Send + Sync {
    fn headers(&self) -> Vec<&'static str>;
    fn format<'a>(
        &'a self,
        client: &'a Arc<JsonRpcClient>,
        ft_metadata_cache: &'a FtMetadataCache,
        proposal: &'a Proposal,
        policy: &'a Policy,
    ) -> BoxFuture<'a, Vec<String>>;
}

pub enum ProposalFormatter {
    Sync(Box<dyn ProposalCsvFormatterSync>),
    Async(Box<dyn ProposalCsvFormatterAsync>),
}

impl ProposalFormatter {
    pub fn headers(&self) -> Vec<&'static str> {
        match self {
            ProposalFormatter::Sync(f) => f.headers(),
            ProposalFormatter::Async(f) => f.headers(),
        }
    }

    pub async fn format(
        &self,
        client: &Arc<JsonRpcClient>,
        ft_metadata_cache: &FtMetadataCache,
        proposal: &Proposal,
        policy: &Policy,
    ) -> Vec<String> {
        match self {
            ProposalFormatter::Sync(f) => f.format(proposal, policy),
            ProposalFormatter::Async(f) => {
                f.format(client, ft_metadata_cache, proposal, policy).await
            }
        }
    }
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

pub async fn fetch_ft_metadata(
    client: &near_jsonrpc_client::JsonRpcClient,
    contract_id: &AccountId,
) -> Result<FtMetadata> {
    let request = RpcQueryRequest {
        block_reference: near_primitives::types::Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: contract_id.clone(),
            method_name: "ft_metadata".to_string(),
            args: FunctionArgs::from(vec![]),
        },
    };

    let response = client.call(request).await?;

    if let QueryResponseKind::CallResult(result) = response.kind {
        let metadata: FtMetadata = serde_json::from_slice(&result.result)?;
        Ok(metadata)
    } else {
        Err(anyhow::anyhow!("Failed to fetch FT metadata"))
    }
}

fn format_ns_timestamp_from_i64(ns: i64) -> Option<String> {
    let secs = ns / 1_000_000_000;
    let nsec = (ns % 1_000_000_000) as u32;

    let datetime_utc = Utc.timestamp_opt(secs, nsec).single()?;

    Some(datetime_utc.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

fn format_ns_timestamp_u64(ns: u64) -> String {
    format_ns_timestamp_from_i64(ns as i64).unwrap_or_else(|| "Invalid timestamp".to_string())
}

fn format_ns_timestamp_str(ns_str: &str) -> Option<String> {
    ns_str
        .parse::<i64>()
        .ok()
        .and_then(format_ns_timestamp_from_i64)
}

#[derive(Debug, Default)]
struct FormattedVotes {
    approved: Vec<String>,
    rejected: Vec<String>, // includes both Reject and Remove
}

fn format_votes(votes: &HashMap<String, Vote>) -> FormattedVotes {
    let mut formatted = FormattedVotes::default();

    for (account, vote) in votes {
        match vote {
            Vote::Approve => formatted.approved.push(account.clone()),
            Vote::Reject | Vote::Remove => formatted.rejected.push(account.clone()),
        }
    }

    formatted.approved.sort();
    formatted.rejected.sort();

    formatted
}

fn extract_from_description(desc: &str, key: &str) -> Option<String> {
    let key_normalized = key.to_lowercase().replace(' ', "");

    // 1) Try parsing JSON
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(desc) {
        for (k, v) in json_val.as_object()? {
            if k.to_lowercase().replace(' ', "") == key_normalized {
                return v
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| Some(v.to_string()));
            }
        }
    }

    // 2) Parse lines split by newlines or <br>
    let lines = desc
        .split(|c| c == '\n' || c == '\r')
        .flat_map(|line| line.split("<br>"))
        .map(|line| line.trim());

    for line in lines {
        if line.starts_with('*') {
            let line_content = line.trim_start_matches('*').trim();
            if let Some(pos) = line_content.find(':') {
                let key_part = line_content[..pos].trim().to_lowercase().replace(' ', "");
                if key_part == key_normalized {
                    let val = line_content[pos + 1..].trim();
                    return Some(val.to_string());
                }
            }
        }
    }

    // Fallback for full description
    if key_normalized == "description" {
        Some(desc.to_string())
    } else {
        None
    }
}

fn get_current_time_nanos() -> U64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos();

    U64::from(nanos as u64)
}

fn get_status_display(status: &ProposalStatus, submission_time: u64, period: u64) -> String {
    match status {
        ProposalStatus::InProgress => {
            let current_time = get_current_time_nanos().0;
            if submission_time + period < current_time {
                format!("Expired")
            } else {
                format!("Pending")
            }
        }
        _ => format!("{:?}", status),
    }
}

impl ProposalCsvFormatterAsync for TransferProposalFormatter {
    fn headers(&self) -> Vec<&'static str> {
        vec![
            "ID",
            "Created Date",
            "Status",
            "Title",
            "Summary",
            "Recipient",
            "Requested Token",
            "Funding Ask",
            "Created by",
            "Notes",
            "Approvers (Approved)",
            "Approvers (Rejected/Remove)",
        ]
    }
    fn format<'a>(
        &'a self,
        client: &'a Arc<JsonRpcClient>,
        ft_metadata_cache: &'a FtMetadataCache,
        proposal: &'a Proposal,
        policy: &'a Policy,
    ) -> BoxFuture<'a, Vec<String>> {
        async move {
            let created_date = format_ns_timestamp_u64(proposal.submission_time.0);

            let title =
                extract_from_description(&proposal.description, "title").unwrap_or_default();
            let summary =
                extract_from_description(&proposal.description, "summary").unwrap_or_default();
            let notes =
                extract_from_description(&proposal.description, "notes").unwrap_or_default();
            let description =
                extract_from_description(&proposal.description, "description").unwrap_or_default();

            let status: String = get_status_display(
                &proposal.status,
                proposal.submission_time.0,
                policy.proposal_period.0,
            );
            let created_by = proposal.proposer.clone();

            let formatted_votes = format_votes(&proposal.votes);

            let kind = &proposal.kind;
            let transfer = kind.get("Transfer");

            let (requested_token, funding_ask, recipient) = if let Some(transfer_val) = transfer {
                let token_id = transfer_val
                    .get("token_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("");
                let amount = transfer_val
                    .get("amount")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let receiver = transfer_val
                    .get("receiver_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                (
                    token_id.to_string(),
                    amount.to_string(),
                    receiver.to_string(),
                )
            } else {
                ("".to_string(), "".to_string(), "".to_string())
            };

            let ft_metadata =
                match get_ft_metadata_cache(&client, &ft_metadata_cache, &requested_token).await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        eprintln!("Error fetching ft metadata: {}", e);
                        FtMetadata::empty()
                    }
                };

            vec![
                proposal.id.to_string(),
                created_date,
                status,
                if !title.is_empty() {
                    title
                } else {
                    description
                },
                summary,
                recipient,
                ft_metadata.symbol,
                normalize_token_amount(&funding_ask, ft_metadata.decimals.into()),
                created_by,
                notes,
                formatted_votes.approved.join(", "),
                formatted_votes.rejected.join(", "),
            ]
        }
        .boxed()
    }
}

fn extract_action_field<'a>(proposal: &'a Proposal, field: &str) -> Option<&'a str> {
    proposal
        .kind
        .get("FunctionCall")?
        .get("actions")?
        .get(0)?
        .get(field)?
        .as_str()
}
fn parse_args<T: DeserializeOwned>(args_base64: &str) -> Option<T> {
    let decoded_bytes = general_purpose::STANDARD.decode(args_base64).ok()?;
    let parsed: T = from_slice(&decoded_bytes).ok()?;
    Some(parsed)
}

fn extract_args(proposal: &Proposal) -> Option<LockupArgs> {
    let args_base64 = extract_action_field(proposal, "args").unwrap_or("");
    parse_args(args_base64)
}

fn normalize_token_amount(raw: &str, decimals: u32) -> String {
    raw.parse::<f64>()
        .map(|v| v / 10f64.powi(decimals as i32))
        .map(|v| format!("{:.5}", v)) // format with 5 decimals (adjust as needed)
        .unwrap_or_default()
}

impl ProposalCsvFormatterSync for LockupProposalFormatter {
    fn headers(&self) -> Vec<&'static str> {
        vec![
            "ID",
            "Created Date",
            "Status",
            "Recipient Account",
            "Amount",
            "Token",
            "Start Date",
            "End Date",
            "Cliff Date",
            "Allow Cancellation",
            "Allow Staking",
            "Created by",
            "Approvers (Approved)",
            "Approvers (Rejected/Remove)",
        ]
    }

    fn format(&self, proposal: &Proposal, policy: &Policy) -> Vec<String> {
        let args_opt = extract_args(proposal);
        let args = args_opt.as_ref();

        let recipient = args
            .and_then(|a| a.owner_account_id.clone())
            .unwrap_or_default();

        let amount = format!(
            "{}",
            normalize_token_amount(&extract_action_field(proposal, "deposit").unwrap_or(""), 24)
        );
        let (start_date, end_date, cliff_date) = match args {
            Some(a) => {
                // Try simple lockup + duration first
                if let (Some(start), Some(duration)) = (&a.lockup_timestamp, &a.release_duration) {
                    let start_date = format_ns_timestamp_str(start).unwrap_or_default();

                    let end_date = match (start.parse::<i64>(), duration.parse::<i64>()) {
                        (Ok(start_ns), Ok(duration_ns)) => {
                            let end_ns = start_ns.checked_add(duration_ns).unwrap_or(0);
                            format_ns_timestamp_str(&end_ns.to_string()).unwrap_or_default()
                        }
                        _ => String::new(),
                    };

                    (start_date, end_date, String::new()) // No cliff date in this format
                } else {
                    // Fallback to nested vesting schedule
                    let vesting = a
                        .vesting_schedule
                        .as_ref()
                        .and_then(|v| v.vesting_schedule.as_ref());

                    let start_date = vesting
                        .and_then(|vs| vs.start_timestamp.as_ref())
                        .map(|s| format_ns_timestamp_str(s).unwrap_or_default())
                        .unwrap_or_default();

                    let end_date = vesting
                        .and_then(|vs| vs.end_timestamp.as_ref())
                        .map(|s| format_ns_timestamp_str(s).unwrap_or_default())
                        .unwrap_or_default();

                    let cliff_date = vesting
                        .and_then(|vs| vs.cliff_timestamp.as_ref())
                        .map(|s| format_ns_timestamp_str(s).unwrap_or_default())
                        .unwrap_or_default();

                    (start_date, end_date, cliff_date)
                }
            }
            None => (String::new(), String::new(), String::new()),
        };

        let allow_cancellation = if args.and_then(|a| a.vesting_schedule.as_ref()).is_some() {
            "yes"
        } else {
            "no"
        }
        .to_string();

        let allow_staking = if args
            .and_then(|a| a.whitelist_account_id.as_ref())
            .map_or(true, |id| id != "lockup-no-whitelist.near")
        {
            "yes"
        } else {
            "no"
        }
        .to_string();

        let formatted_votes = format_votes(&proposal.votes);

        let created_date: String = format_ns_timestamp_u64(proposal.submission_time.0);

        let status: String = get_status_display(
            &proposal.status,
            proposal.submission_time.0,
            policy.proposal_period.0,
        );
        let created_by = proposal.proposer.clone();
        vec![
            proposal.id.to_string(),
            created_date,
            status,
            recipient,
            amount,
            "NEAR".to_string(),
            start_date,
            end_date,
            cliff_date,
            allow_cancellation,
            allow_staking,
            created_by,
            formatted_votes.approved.join(", "),
            formatted_votes.rejected.join(", "),
        ]
    }
}

impl ProposalCsvFormatterSync for DefaultFormatter {
    fn headers(&self) -> Vec<&'static str> {
        vec![
            "ID",
            "Created Date",
            "Status",
            "Description",
            "Kind",
            "Created by",
            "Approvers (Approved)",
            "Approvers (Rejected/Remove)",
        ]
    }
    fn format(&self, proposal: &Proposal, policy: &Policy) -> Vec<String> {
        let formatted_votes = format_votes(&proposal.votes);
        let status: String = get_status_display(
            &proposal.status,
            proposal.submission_time.0,
            policy.proposal_period.0,
        );
        let kind = proposal.kind.clone();
        let created_date: String = format_ns_timestamp_u64(proposal.submission_time.0);
        let created_by = proposal.proposer.clone();
        vec![
            proposal.id.to_string(),
            created_date,
            status,
            proposal.description.clone(),
            kind.to_string(),
            created_by,
            formatted_votes.approved.join(", "),
            formatted_votes.rejected.join(", "),
        ]
    }
}

impl ProposalCsvFormatterSync for StakeDelegationProposalFormatter {
    fn headers(&self) -> Vec<&'static str> {
        vec![
            "ID",
            "Created Date",
            "Status",
            "Type",
            "Amount",
            "Token",
            "Validator",
            "Created by",
            "Notes",
            "Approvers (Approved)",
            "Approvers (Rejected/Remove)",
        ]
    }

    fn format(&self, proposal: &Proposal, policy: &Policy) -> Vec<String> {
        let notes = extract_from_description(&proposal.description, "notes").unwrap_or_default();
        let receiver_id = proposal
            .kind
            .get("FunctionCall")
            .and_then(|fc| fc.get("receiver_id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let method_name = extract_action_field(proposal, "method_name").unwrap_or("");

        // returning empty for lockup related stake requests
        if receiver_id.contains("lockup.near") {
            return vec![];
        }
        // Determine proposal type and amount
        let (proposal_type, amount) = match method_name {
            "unstake" => {
                let amt = extract_action_field(proposal, "args")
                    .and_then(|args_b64| parse_args::<serde_json::Value>(args_b64))
                    .and_then(|json| {
                        json.get("amount")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                ("Unstake".to_string(), amt)
            }
            "deposit_and_stake" => {
                let amt = extract_action_field(proposal, "deposit")
                    .unwrap_or("")
                    .to_string();
                ("Stake".to_string(), amt)
            }
            "withdraw_all" => {
                let amt =
                    extract_from_description(&proposal.description, "Amount").unwrap_or_default();
                ("Withdraw".to_string(), amt)
            }
            _ => ("Unknown".to_string(), "".to_string()),
        };
        let parsed_amount = format!("{}", normalize_token_amount(&amount, 24));

        let formatted_votes = format_votes(&proposal.votes);
        let status: String = get_status_display(
            &proposal.status,
            proposal.submission_time.0,
            policy.proposal_period.0,
        );
        let created_date: String = format_ns_timestamp_u64(proposal.submission_time.0);
        vec![
            proposal.id.to_string(),
            created_date,
            status,
            proposal_type,
            parsed_amount,
            "NEAR".to_string(),
            receiver_id.to_string(),
            proposal.proposer.clone(),
            notes,
            formatted_votes.approved.join(", "),
            formatted_votes.rejected.join(", "),
        ]
    }
}

impl ProposalCsvFormatterAsync for AssetExchangeProposalFormatter {
    fn headers(&self) -> Vec<&'static str> {
        vec![
            "ID",
            "Created Date",
            "Status",
            "Send Amount",
            "Send Token",
            "Receive Amount",
            "Receive Token",
            "Created By",
            "Notes",
            "Approvers (Approved)",
            "Approvers (Rejected/Remove)",
        ]
    }

    fn format<'a>(
        &'a self,
        client: &'a Arc<JsonRpcClient>,
        ft_metadata_cache: &'a FtMetadataCache,
        proposal: &'a Proposal,
        policy: &'a Policy,
    ) -> BoxFuture<'a, Vec<String>> {
        async move {
            let proposal_id = proposal.id.to_string();
            let created_by = proposal.proposer.clone();
            let formatted_votes = format_votes(&proposal.votes);

            let send_amount =
                extract_from_description(&proposal.description, "amountIn").unwrap_or_default();
            let send_token =
                extract_from_description(&proposal.description, "tokenIn").unwrap_or_default();
            let receive_token =
                extract_from_description(&proposal.description, "tokenOut").unwrap_or_default();
            let receive_amount =
                extract_from_description(&proposal.description, "amountOut").unwrap_or_default();
            let notes =
                extract_from_description(&proposal.description, "notes").unwrap_or_default();
            let status: String = get_status_display(
                &proposal.status,
                proposal.submission_time.0,
                policy.proposal_period.0,
            );
            let ft_meta_send =
                match get_ft_metadata_cache(&client, &ft_metadata_cache, &send_token).await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        eprintln!("Error fetching send token ft metadata: {}", e);
                        FtMetadata::empty()
                    }
                };

            let ft_meta_receive =
                match get_ft_metadata_cache(&client, &ft_metadata_cache, &receive_token).await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        eprintln!("Error fetching receive token ft metadata: {}", e);
                        FtMetadata::empty()
                    }
                };
            let created_date: String = format_ns_timestamp_u64(proposal.submission_time.0);
            vec![
                proposal_id,
                created_date,
                status,
                send_amount,
                ft_meta_send.symbol.clone(),
                receive_amount,
                ft_meta_receive.symbol.clone(),
                created_by,
                notes,
                formatted_votes.approved.join(", "),
                formatted_votes.rejected.join(", "),
            ]
        }
        .boxed()
    }
}
