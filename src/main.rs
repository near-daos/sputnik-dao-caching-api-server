#[macro_use]
extern crate rocket;
mod cache;
mod csv_view;
mod filters;
mod persistence;
mod rpc_client;
pub mod scraper;
#[cfg(test)]
#[path = "./tests/integration_test.rs"]
mod tests;

use csv_view::ProposalCsvView;
use near_primitives::types::AccountId;
use rocket::State;
use rocket::response::content::RawText;
use rocket_cors::{AllowedOrigins, CorsOptions};

use rocket::http::{Accept, Status};
use rocket::response::Responder;
use rocket::serde::json::Json;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cache::{ProposalCache, ProposalStore, get_latest_dao_cache, get_latest_proposal_cache};
use filters::ProposalFilters;
use persistence::{CachePersistence, read_cache_from_file};
use scraper::{Proposal, TxMetadata};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct ProposalOutput {
    #[serde(flatten)]
    proposal: Proposal,
    txs_log: Vec<TxMetadata>,
}

#[derive(Responder)]
enum ResponseOptions<T> {
    #[response(content_type = "application/json")]
    Json(Json<T>),
    #[response(content_type = "text/csv")]
    Csv(RawText<String>),
}

#[get("/proposals/<dao_id>?<filters..>")]
async fn get_dao_proposals(
    dao_id: &str,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
    accept: Option<&Accept>,
) -> Result<ResponseOptions<Vec<Proposal>>, Status> {
    let dao_id: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();

    let cached = get_latest_dao_cache(&client, &store, &dao_id)
        .await
        .map_err(|_| Status::NotFound)?;
    let filtered_proposals = filters.filter_proposals(cached.proposals, &cached.policy);

    if accept
        .map(|a| a.preferred().media_type() == &rocket::http::MediaType::CSV)
        .unwrap_or(false)
    {
        let mut wtr = csv::Writer::from_writer(vec![]);
        for p in filtered_proposals {
            wtr.serialize(ProposalCsvView::from(p)).unwrap();
        }
        let csv_data = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
        return Ok(ResponseOptions::Csv(RawText(csv_data)));
    }
    Ok(ResponseOptions::Json(Json(filtered_proposals)))
}

#[get("/proposals/<dao_id>/<proposal_id>")]
async fn get_specific_proposal(
    dao_id: &str,
    proposal_id: u64,
    cache: &State<ProposalCache>,
    accept: Option<&Accept>,
) -> Result<ResponseOptions<ProposalOutput>, Status> {
    let dao_id_account: AccountId = dao_id.parse().map_err(|_| Status::BadRequest)?;
    let client = rpc_client::get_rpc_client();
    let proposal_cached = get_latest_proposal_cache(&client, cache, &dao_id_account, proposal_id)
        .await
        .map_err(|_| Status::NotFound)?;

    if accept
        .map(|a| a.preferred().media_type() == &rocket::http::MediaType::CSV)
        .unwrap_or(false)
    {
        let mut wtr = csv::Writer::from_writer(vec![]);
        let mut p_csv = ProposalCsvView::from(proposal_cached.proposal);
        p_csv.txs_log = Some(serde_json::to_string(&proposal_cached.txs_log).unwrap_or_default());
        wtr.serialize(p_csv).unwrap();

        let csv_data = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
        return Ok(ResponseOptions::Csv(RawText(csv_data)));
    }
    Ok(ResponseOptions::Json(Json(ProposalOutput {
        proposal: proposal_cached.proposal,
        txs_log: proposal_cached.txs_log,
    })))
}

#[launch]
pub fn rocket() -> _ {
    let proposals_store: ProposalStore = Arc::new(RwLock::new(HashMap::new()));
    let proposal_cache: ProposalCache =
        read_cache_from_file().unwrap_or_else(|_| Arc::new(RwLock::new(HashMap::new())));

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
        .mount("/", routes![get_dao_proposals, get_specific_proposal])
        .attach(cache_persistence)
        .attach(cors)
        .configure(
            rocket::Config::figment()
                .merge(("port", 5001))
                .merge(("address", "0.0.0.0")),
        )
}
