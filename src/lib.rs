#[macro_use]
extern crate rocket;
mod cache;
mod csv_view;
pub mod filters;
mod persistence;
pub mod rpc_client;
pub mod scraper;

use near_primitives::types::AccountId;
use rocket::State;

use rocket::serde::json::Json;
use rocket_cors::{AllowedOrigins, CorsOptions};

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cache::{
    FtMetadataCache, ProposalCache, ProposalStore, get_latest_dao_cache, get_latest_proposal_cache,
};

// Helper function to get cached data with consistent error handling
async fn get_cached_data(
    dao_id: &AccountId,
    client: &Arc<near_jsonrpc_client::JsonRpcClient>,
    store: &ProposalStore,
) -> Result<cache::CachedProposals, Status> {
    match get_latest_dao_cache(client, store, dao_id).await {
        Ok(cache) => Ok(cache),
        Err(e) => {
            eprintln!("Failed to get latest DAO cache: {:?}", e);
            Err(Status::NotFound)
        }
    }
}
use filters::{ProposalFilters, categories};
use persistence::{CachePersistence, read_cache_from_file};
use scraper::{
    AssetExchangeInfo, AssetExchangeProposalFormatter, DefaultFormatter, LockupInfo,
    LockupProposalFormatter, PaymentInfo, Proposal, ProposalCsvFormatterAsync,
    ProposalCsvFormatterSync, ProposalType, StakeDelegationInfo, StakeDelegationProposalFormatter,
    TransferProposalFormatter, TxMetadata,
};

use rocket::Request;
use rocket::http::{ContentType, Header, Status};
use rocket::response::{Responder, Response};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Serialize, Deserialize, Debug)]
pub struct ProposalOutput {
    #[serde(flatten)]
    pub proposal: Proposal,
    pub txs_log: Vec<TxMetadata>,
}

#[derive(Serialize)]
pub struct PaginatedProposals {
    pub proposals: Vec<Proposal>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Serialize)]
pub struct ProposersResponse {
    pub proposers: Vec<String>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct ApproversResponse {
    pub approvers: Vec<String>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct RecipientsResponse {
    pub recipients: Vec<String>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct RequestedTokensResponse {
    pub requested_tokens: Vec<String>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct ValidatorsResponse {
    pub validators: Vec<String>,
    pub total: usize,
}

#[get("/proposals/<dao_id>?<filters..>")]
pub async fn get_proposals(
    dao_id: &str,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
    ft_metadata_cache: &State<FtMetadataCache>,
) -> Result<Json<PaginatedProposals>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    // Get cached data
    let cached = get_cached_data(&dao_id, &client, &store).await?;

    // Apply filters
    let filtered_proposals = filters
        .filter_proposals_async(cached.proposals, &cached.policy, &ft_metadata_cache)
        .await
        .map_err(|e| {
            eprintln!("Error filtering proposals: {}", e);
            Status::InternalServerError
        })?;
    let total = filtered_proposals.len();

    // Handle pagination
    let proposals = match (filters.page, filters.page_size) {
        (Some(page), Some(page_size)) => {
            // Frontend sends 0-based page numbers
            let start = page * page_size;
            let end = start + page_size;

            if start < total {
                filtered_proposals[start..filtered_proposals.len().min(end)].to_vec()
            } else {
                vec![]
            }
        }
        _ => filtered_proposals,
    };

    Ok(Json(PaginatedProposals {
        proposals,
        total,
        page: filters.page.unwrap_or(0),
        page_size: filters.page_size.unwrap_or(total),
    }))
}

#[get("/proposal/<dao_id>/<proposal_id>")]
pub async fn get_specific_proposal(
    dao_id: &str,
    proposal_id: u64,
    cache: &State<ProposalCache>,
) -> Result<Json<ProposalOutput>, Status> {
    let dao_id_account: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();
    let proposal_cached = get_latest_proposal_cache(&client, cache, &dao_id_account, proposal_id)
        .await
        .map_err(|_| Status::NotFound)?;

    Ok(Json(ProposalOutput {
        proposal: proposal_cached.proposal,
        txs_log: proposal_cached.txs_log,
    }))
}

#[get("/proposals/<dao_id>/proposers")]
pub async fn get_dao_proposers(
    dao_id: &str,
    store: &State<ProposalStore>,
) -> Result<Json<ProposersResponse>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = get_cached_data(&dao_id, &client, &store).await?;

    // Extract unique proposers from all proposals
    let mut proposers: std::collections::HashSet<String> = std::collections::HashSet::new();
    for proposal in &cached.proposals {
        proposers.insert(proposal.proposer.clone());
    }

    let mut proposers_vec: Vec<String> = proposers.into_iter().collect();
    proposers_vec.sort_unstable(); // Sort alphabetically for consistent ordering

    let total = proposers_vec.len();

    Ok(Json(ProposersResponse {
        proposers: proposers_vec,
        total,
    }))
}

#[get("/proposals/<dao_id>/approvers")]
pub async fn get_dao_approvers(
    dao_id: &str,
    store: &State<ProposalStore>,
) -> Result<Json<ApproversResponse>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = get_cached_data(&dao_id, &client, &store).await?;

    // Extract unique approvers from all proposals
    let mut approvers: std::collections::HashSet<String> = std::collections::HashSet::new();
    for proposal in &cached.proposals {
        // Add all voters from the votes HashMap
        for (voter, _) in &proposal.votes {
            approvers.insert(voter.clone());
        }
    }

    let mut approvers_vec: Vec<String> = approvers.into_iter().collect();
    approvers_vec.sort_unstable(); // Sort alphabetically for consistent ordering

    let total = approvers_vec.len();

    Ok(Json(ApproversResponse {
        approvers: approvers_vec,
        total,
    }))
}

#[get("/proposals/<dao_id>/recipients")]
pub async fn get_dao_recipients(
    dao_id: &str,
    store: &State<ProposalStore>,
) -> Result<Json<RecipientsResponse>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = get_cached_data(&dao_id, &client, &store).await?;

    // Extract unique recipients from transfer proposals only
    let mut recipients: std::collections::HashSet<String> = std::collections::HashSet::new();
    for proposal in &cached.proposals {
        // Check if this is a transfer proposal
        if let Some(payment_info) = scraper::PaymentInfo::from_proposal(proposal) {
            recipients.insert(payment_info.receiver);
        }
    }

    let mut recipients_vec: Vec<String> = recipients.into_iter().collect();
    recipients_vec.sort_unstable(); // Sort alphabetically for consistent ordering

    let total = recipients_vec.len();

    Ok(Json(RecipientsResponse {
        recipients: recipients_vec,
        total,
    }))
}

#[get("/proposals/<dao_id>/requested-tokens")]
pub async fn get_dao_requested_tokens(
    dao_id: &str,
    store: &State<ProposalStore>,
) -> Result<Json<RequestedTokensResponse>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = get_cached_data(&dao_id, &client, &store).await?;

    // Extract unique request tokens from transfer proposals only
    let mut request_tokens: std::collections::HashSet<String> = std::collections::HashSet::new();
    for proposal in &cached.proposals {
        // Check if this is a transfer proposal
        if let Some(payment_info) = scraper::PaymentInfo::from_proposal(proposal) {
            // Map empty string to "near" for NEAR tokens
            let token = if payment_info.token.is_empty() {
                "near".to_string()
            } else {
                payment_info.token
            };
            request_tokens.insert(token);
        }
    }

    let mut request_tokens_vec: Vec<String> = request_tokens.into_iter().collect();
    request_tokens_vec.sort_unstable(); // Sort alphabetically for consistent ordering

    let total = request_tokens_vec.len();

    Ok(Json(RequestedTokensResponse {
        requested_tokens: request_tokens_vec,
        total,
    }))
}

#[get("/proposals/<dao_id>/validators")]
pub async fn get_dao_validators(
    dao_id: &str,
    store: &State<ProposalStore>,
) -> Result<Json<ValidatorsResponse>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = get_cached_data(&dao_id, &client, &store).await?;

    // Extract unique validators from stake delegation proposals only
    let mut validators: std::collections::HashSet<String> = std::collections::HashSet::new();
    let staking_pool_cache = cache::StakingPoolCache::new();

    for proposal in &cached.proposals {
        // Check if this is a stake delegation proposal
        if let Some(stake_info) = scraper::StakeDelegationInfo::from_proposal(proposal) {
            // For lockup accounts, we need to resolve the validator via RPC
            if stake_info.validator.contains(".lockup.near") {
                // This is a lockup account, resolve the validator
                if let Some(validator) = staking_pool_cache
                    .get_staking_pool_account_id(&client, &stake_info.validator)
                    .await
                {
                    validators.insert(validator);
                } else {
                    // If RPC call fails, still include the lockup account as fallback
                    validators.insert(stake_info.validator);
                }
            } else {
                // Direct validator account
                validators.insert(stake_info.validator);
            }
        }
    }

    let mut validators_vec: Vec<String> = validators.into_iter().collect();
    validators_vec.sort_unstable(); // Sort alphabetically for consistent ordering

    let total = validators_vec.len();

    Ok(Json(ValidatorsResponse {
        validators: validators_vec,
        total,
    }))
}

pub struct CsvFile {
    pub content: String,
    pub filename: String,
}

impl<'r> Responder<'r, 'static> for CsvFile {
    fn respond_to(self, _req: &'r Request<'_>) -> rocket::response::Result<'static> {
        Response::build()
            .header(ContentType::new("text", "csv"))
            .header(Header::new(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", self.filename),
            ))
            .sized_body(self.content.len(), Cursor::new(self.content))
            .ok()
    }
}

#[get("/csv/proposals/<dao_id>?<filters..>")]
pub async fn csv_proposals(
    dao_id: &str,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
    ft_metadata_cache: &State<FtMetadataCache>,
) -> Result<CsvFile, Status> {
    if dao_id.is_empty() {
        return Err(Status::BadRequest);
    }

    let client = rpc_client::get_rpc_client();
    let dao_id_account = dao_id.parse().map_err(|_| Status::BadRequest)?;

    // Get cached data
    let cached = get_latest_dao_cache(&client, &store, &dao_id_account)
        .await
        .map_err(|_| Status::NotFound)?;

    let proposals = filters
        .filter_proposals_async(cached.proposals, &cached.policy, &ft_metadata_cache)
        .await
        .map_err(|e| {
            eprintln!("Error filtering proposals for CSV: {}", e);
            Status::InternalServerError
        })?;

    // Check if DAO has a lockup account (for payments or stake delegation category)
    let has_lockup_account = match filters.category.as_deref() {
        Some(categories::PAYMENTS) | Some(categories::STAKE_DELEGATION) => {
            rpc_client::account_to_lockup(&client, dao_id)
                .await
                .is_some()
        }
        _ => false,
    };

    let mut wtr = csv::Writer::from_writer(vec![]);

    // Helper functions to write CSV records with error handling
    let write_headers = |wtr: &mut csv::Writer<Vec<u8>>, headers: &[&str]| -> Result<(), Status> {
        wtr.write_record(headers)
            .map_err(|_| Status::InternalServerError)
    };

    let write_record = |wtr: &mut csv::Writer<Vec<u8>>, record: &[String]| -> Result<(), Status> {
        wtr.write_record(record)
            .map_err(|_| Status::InternalServerError)
    };

    match filters.category.as_deref() {
        Some(categories::PAYMENTS) => {
            let extracted = filters.filter_and_extract::<PaymentInfo>(proposals);
            let formatter = TransferProposalFormatter;
            let mut headers = formatter.headers();
            if !has_lockup_account {
                if let Some(index) = headers.iter().position(|&h| h == "Treasury Wallet") {
                    headers.remove(index);
                }
            }
            write_headers(&mut wtr, &headers)?;
            for (proposal, payment_info) in extracted {
                let mut record = formatter
                    .format(
                        &client,
                        &ft_metadata_cache,
                        &proposal,
                        &cached.policy,
                        &payment_info,
                    )
                    .await;
                if record.is_empty() {
                    continue;
                }
                if !has_lockup_account && record.len() > 3 {
                    record.remove(3);
                }
                write_record(&mut wtr, &record)?;
            }
        }
        Some(categories::LOCKUP) => {
            let extracted = filters.filter_and_extract::<LockupInfo>(proposals);
            let formatter = LockupProposalFormatter;
            let headers = formatter.headers();
            write_headers(&mut wtr, &headers)?;
            for (proposal, lockup_info) in extracted {
                let record = formatter.format(&proposal, &cached.policy, &lockup_info);
                if record.is_empty() {
                    continue;
                }
                write_record(&mut wtr, &record)?;
            }
        }
        Some(categories::ASSET_EXCHANGE) => {
            let extracted = filters.filter_and_extract::<AssetExchangeInfo>(proposals);
            let formatter = AssetExchangeProposalFormatter;
            let headers = formatter.headers();
            write_headers(&mut wtr, &headers)?;
            for (proposal, asset_info) in extracted {
                let record = formatter
                    .format(
                        &client,
                        &ft_metadata_cache,
                        &proposal,
                        &cached.policy,
                        &asset_info,
                    )
                    .await;
                if record.is_empty() {
                    continue;
                }
                write_record(&mut wtr, &record)?;
            }
        }
        Some(categories::STAKE_DELEGATION) => {
            let extracted = filters.filter_and_extract::<StakeDelegationInfo>(proposals);
            let formatter = StakeDelegationProposalFormatter;
            let mut headers = formatter.headers();
            if !has_lockup_account {
                if let Some(index) = headers.iter().position(|&h| h == "Treasury Wallet") {
                    headers.remove(index);
                }
            }
            write_headers(&mut wtr, &headers)?;
            for (proposal, stake_info) in extracted {
                let mut record = formatter
                    .format(
                        &client,
                        &ft_metadata_cache,
                        &proposal,
                        &cached.policy,
                        &stake_info,
                    )
                    .await;
                if record.is_empty() {
                    continue;
                }
                if !has_lockup_account && record.len() > 3 {
                    record.remove(3);
                }
                write_record(&mut wtr, &record)?;
            }
        }
        _ => {
            // Default: use the old logic for other categories
            let formatter = DefaultFormatter;
            let headers = formatter.headers();
            write_headers(&mut wtr, &headers)?;
            for proposal in proposals {
                let record = formatter.format(&proposal, &cached.policy, &());
                if record.is_empty() {
                    continue;
                }
                write_record(&mut wtr, &record)?;
            }
        }
    }

    let data = String::from_utf8(wtr.into_inner().map_err(|_| Status::InternalServerError)?)
        .map_err(|_| Status::InternalServerError)?;

    Ok(CsvFile {
        content: data,
        filename: format!("proposals_{}.csv", dao_id),
    })
}

// This is the function your main.rs and tests should call!
pub fn rocket() -> rocket::Rocket<rocket::Build> {
    let proposals_store: ProposalStore = Arc::new(RwLock::new(HashMap::new()));
    let proposal_cache: ProposalCache =
        read_cache_from_file().unwrap_or_else(|_| Arc::new(RwLock::new(HashMap::new())));

    let ft_metadata_cache: FtMetadataCache = Arc::new(RwLock::new(HashMap::new()));

    let cache_persistence = CachePersistence {
        proposal_cache: proposal_cache.clone(),
    };

    // Configure CORS
    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::some_regex(&[
            r"https?://.*\.near\.page",
            r"https?://near\.social",
            r"https?://near\.org",
            r"https?://localhost:3000",
            r"https?://near-treasury\.vercel\.app",
            r"https?://app\.neartreasury\.com",
            r"https?://near-treasury-sigma\.vercel\.app",
            r"https?://localhost:8080",
            r"https?://localhost:5001",
            r"https?://127\.0\.0\.1:8080",
            r"https?://sputnik-indexer-divine-fog-3863\.fly\.dev",
            r"https?://sputnik-indexer\.fly\.dev",
        ]))
        .allow_credentials(true)
        .to_cors()
        .expect("Failed to create CORS fairing");

    rocket::build()
        .manage(proposals_store)
        .manage(proposal_cache)
        .manage(ft_metadata_cache)
        .mount(
            "/",
            routes![
                get_proposals,
                get_specific_proposal,
                get_dao_proposers,
                get_dao_approvers,
                get_dao_recipients,
                get_dao_requested_tokens,
                get_dao_validators,
                csv_proposals
            ],
        )
        .attach(cache_persistence)
        .attach(cors)
        .configure(
            rocket::Config::figment()
                .merge((
                    "port",
                    std::env::var("PORT")
                        .unwrap_or("5001".to_string())
                        .parse::<u16>()
                        .unwrap_or(5001),
                ))
                .merge(("address", "0.0.0.0")),
        )
}
