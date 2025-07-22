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
use filters::{ProposalFilters, categories};
use persistence::{CachePersistence, read_cache_from_file};
use scraper::{
    AssetExchangeInfo, AssetExchangeProposalFormatter, DefaultFormatter, LockupInfo,
    LockupProposalFormatter, PaymentInfo, Proposal, ProposalCsvFormatterAsync,
    ProposalCsvFormatterSync, StakeDelegationInfo, StakeDelegationProposalFormatter,
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

#[get("/proposals/<dao_id>?<filters..>")]
pub async fn get_dao_proposals(
    dao_id: &str,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
) -> Result<Json<PaginatedProposals>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    // Get cached data
    let cached = match get_latest_dao_cache(&client, &store, &dao_id).await {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("Failed to get latest DAO cache: {:?}", e);
            return Err(Status::NotFound);
        }
    };

    // Apply filters
    let filtered_proposals = filters.filter_proposals(cached.proposals, &cached.policy);
    let total = filtered_proposals.len();

    // Handle pagination
    let paginated: Vec<Proposal> = match (filters.page, filters.page_size) {
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
        _ => filtered_proposals.clone(),
    };

    Ok(Json(PaginatedProposals {
        proposals: paginated,
        total,
        page: filters.page.unwrap_or(1),
        page_size: filters.page_size.unwrap_or(total),
    }))
}

#[get("/proposals/<dao_id>/<proposal_id>")]
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
            let extracted = filters.filter_and_extract::<PaymentInfo>(cached.proposals);
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
            let extracted = filters.filter_and_extract::<LockupInfo>(cached.proposals);
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
            let extracted = filters.filter_and_extract::<AssetExchangeInfo>(cached.proposals);
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
            let extracted = filters.filter_and_extract::<StakeDelegationInfo>(cached.proposals);
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
            for proposal in filters.filter_proposals(cached.proposals, &cached.policy) {
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
        .allowed_origins(AllowedOrigins::some_exact(&[
            "http://localhost:8080",
            "http://localhost:5001",
            "http://127.0.0.1:8080",
            "https://sputnik-indexer-divine-fog-3863.fly.dev",
            "https://sputnik-indexer.fly.dev",
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
            routes![get_dao_proposals, get_specific_proposal, csv_proposals],
        )
        .attach(cache_persistence)
        .attach(cors)
        .configure(
            rocket::Config::figment()
                .merge(("port", 5001))
                .merge(("address", "0.0.0.0")),
        )
}
