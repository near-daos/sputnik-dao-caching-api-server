#[macro_use]
extern crate rocket;
mod cache;
mod filters;
mod persistence;
mod rpc_client;
pub mod scraper;
#[cfg(test)]
#[path = "./tests/integration_test.rs"]
mod tests;

use near_primitives::types::AccountId;
use rocket::State;

use rocket::http::Status;
use rocket::serde::json::Json;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cache::{ProposalCache, ProposalStore, get_latest_dao_cache, get_latest_proposal_cache};
use filters::ProposalFilters;
use persistence::{CachePersistence, read_cache_from_file};
use scraper::{Proposal, TxMetadata};

use serde::Serialize;

#[derive(Serialize, Debug)]
struct ProposalOutput {
    #[serde(flatten)]
    proposal: Proposal,
    txs_log: Vec<TxMetadata>,
}

#[get("/proposals/<dao_id>?<filters..>")]
async fn get_dao_proposals(
    dao_id: &str,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
) -> Result<Json<Vec<Proposal>>, Status> {
    let dao_id: AccountId = dao_id.parse().unwrap();
    let client = rpc_client::get_rpc_client();

    let cached = get_latest_dao_cache(&client, &store, &dao_id)
        .await
        .map_err(|_| Status::NotFound)?;
    let filtered_proposals = filters.filter_proposals(cached.proposals, &cached.policy);
    Ok(Json(filtered_proposals))
}

#[get("/proposals/<dao_id>/<proposal_id>")]
async fn get_specific_proposal(
    dao_id: &str,
    proposal_id: u64,
    cache: &State<ProposalCache>,
) -> Result<Json<ProposalOutput>, Status> {
    let dao_id_account: AccountId = dao_id.parse().unwrap();
    let client = rpc_client::get_rpc_client();
    let proposal_cached = get_latest_proposal_cache(&client, cache, &dao_id_account, proposal_id)
        .await
        .map_err(|_| Status::NotFound)?;
    Ok(Json(ProposalOutput {
        proposal: proposal_cached.proposal,
        txs_log: proposal_cached.txs_log,
    }))
}

#[launch]
pub fn rocket() -> _ {
    let proposals_store: ProposalStore = Arc::new(RwLock::new(HashMap::new()));
    let proposal_cache: ProposalCache =
        read_cache_from_file().unwrap_or_else(|_| Arc::new(RwLock::new(HashMap::new())));

    let cache_persistence = CachePersistence {
        proposal_cache: proposal_cache.clone(),
    };

    rocket::build()
        .manage(proposals_store)
        .manage(proposal_cache)
        .mount("/", routes![get_dao_proposals, get_specific_proposal])
        .attach(cache_persistence)
        .configure(
            rocket::Config::figment()
                .merge(("port", 5001))
                .merge(("address", "0.0.0.0")),
        )
}
