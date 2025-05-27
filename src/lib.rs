#[macro_use]
extern crate rocket;
mod cache;
mod csv_view;
mod filters;
mod persistence;
mod rpc_client;
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
use filters::ProposalFilters;
use persistence::{CachePersistence, read_cache_from_file};
use scraper::{
    AssetExchangeProposalFormatter, DefaultFormatter, LockupProposalFormatter, Proposal,
    ProposalFormatter, StakeDelegationProposalFormatter, TransferProposalFormatter, TxMetadata,
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

#[get("/proposals/<dao_id>?<filters..>")]
pub async fn get_dao_proposals(
    dao_id: &str,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
) -> Result<Json<Vec<Proposal>>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = match get_latest_dao_cache(&client, &store, &dao_id).await {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("Failed to get latest DAO cache: {:?}", e);
            return Err(Status::NotFound);
        }
    };
    let filtered_proposals = filters.filter_proposals(cached.proposals, &cached.policy);

    Ok(Json(filtered_proposals))
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
    let cached = get_latest_dao_cache(
        &client,
        &store,
        &dao_id.parse().map_err(|_| Status::BadRequest)?,
    )
    .await
    .map_err(|_| Status::NotFound)?;

    let filtered_proposals = filters.filter_proposals(cached.proposals, &cached.policy);

    let proposal_types = filters.proposal_type.as_deref().unwrap_or(&[]);
    let keyword_lower = filters.keyword.as_ref().map(|k| k.to_lowercase());

    let is_type = |t: &str| proposal_types.iter().any(|pt| pt == t);

    let formatter = match keyword_lower.as_deref() {
        Some(keyword) if is_type("FunctionCall") && keyword.contains("lockup") => {
            ProposalFormatter::Sync(Box::new(LockupProposalFormatter))
        }
        Some(keyword) if is_type("FunctionCall") && keyword.contains("asset") => {
            ProposalFormatter::Async(Box::new(AssetExchangeProposalFormatter))
        }
        Some(keyword) if is_type("FunctionCall") && keyword.contains("stake") => {
            ProposalFormatter::Sync(Box::new(StakeDelegationProposalFormatter))
        }
        _ if is_type("Transfer") => ProposalFormatter::Async(Box::new(TransferProposalFormatter)),
        _ => ProposalFormatter::Sync(Box::new(DefaultFormatter)),
    };

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(&formatter.headers()).unwrap();

    for proposal in filtered_proposals {
        let record = formatter
            .format(&client, &ft_metadata_cache, &proposal)
            .await;
        wtr.write_record(&record).unwrap();
    }

    let data = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
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
