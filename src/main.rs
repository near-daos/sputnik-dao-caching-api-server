#[macro_use]
extern crate rocket;
mod rpc_client;
mod scraper;

use near_jsonrpc_client::JsonRpcClient;
use near_primitives::types::AccountId;
use rocket::State;
use rocket::form::FromFormField;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use scraper::{
    Proposal, ProposalStatus, StateVersion, fetch_contract_version, fetch_policy, fetch_proposal,
    fetch_proposals,
};

struct CachedProposals {
    proposals: Vec<Proposal>,
    policy: scraper::Policy,
    last_updated: Instant,
    version: StateVersion,
}

struct CachedProposal {
    proposal: Proposal,
    last_updated: Instant,
    version: StateVersion,
}

type ProposalStore = Arc<RwLock<HashMap<String, CachedProposals>>>;
type ProposalCache = Arc<RwLock<HashMap<(String, u64), CachedProposal>>>;

#[derive(Deserialize, FromFormField)]
enum SortBy {
    CreationTime,
    ExpiryTime,
}

#[get(
    "/proposals/<dao_id>?<status>&<keyword>&<proposer>&<proposal_type>&<min_votes>&<sort_by>&<sort_direction>"
)]
async fn get_dao_proposals(
    dao_id: String,
    status: Option<ProposalStatus>,
    keyword: Option<String>,
    proposer: Option<String>,
    proposal_type: Option<String>,
    min_votes: Option<usize>,
    sort_by: Option<SortBy>,
    sort_direction: Option<String>,
    store: &State<ProposalStore>,
) -> Json<Vec<Proposal>> {
    let dao_id: AccountId = dao_id.parse().unwrap();

    // Check if we have a fresh cache entry
    let should_fetch = {
        let store_read = store
            .read()
            .expect("Failed to acquire read lock on proposal store");
        !store_read.contains_key(dao_id.as_str())
            || store_read.get(dao_id.as_str()).map_or(true, |cached| {
                cached.last_updated.elapsed() > Duration::from_secs(5)
            })
    };

    // Fetch new data if needed
    if should_fetch {
        let client = rpc_client::get_rpc_client();
        let proposal_outputs = fetch_proposals(&client, &dao_id).await.unwrap();
        let policy = fetch_policy(&client, &dao_id).await.unwrap();
        let version = fetch_contract_version(&client, &dao_id).await.unwrap();
        // Update the cache
        let mut store_write = store
            .write()
            .expect("Failed to acquire write lock on proposal store");
        store_write.insert(
            dao_id.to_string(),
            CachedProposals {
                proposals: proposal_outputs,
                policy,
                last_updated: Instant::now(),
                version,
            },
        );
    }

    // Process the data (either fresh or from cache)
    let (proposal_outputs, policy) = {
        let store_read = store
            .read()
            .expect("Failed to acquire read lock on proposal store");
        let cached = store_read
            .get(dao_id.as_str())
            .expect("Cache entry should exist");
        (cached.proposals.clone(), cached.policy.clone())
    };

    let mut filtered_proposals = proposal_outputs
        .into_iter()
        .filter(|proposal| {
            let status_match = status
                .as_ref()
                .map(|s| proposal.status == *s)
                .unwrap_or(true);

            let keyword_match = keyword
                .as_ref()
                .map(|k| {
                    proposal
                        .description
                        .to_lowercase()
                        .contains(&k.to_lowercase())
                })
                .unwrap_or(true);

            let proposer_match = proposer
                .as_ref()
                .map(|p| proposal.proposer == *p)
                .unwrap_or(true);

            let proposal_type_match = proposal_type
                .as_ref()
                .map(|pt| {
                    proposal
                        .kind
                        .to_string()
                        .to_lowercase()
                        .contains(&pt.to_lowercase())
                })
                .unwrap_or(true);

            let votes_match = min_votes
                .map(|min| proposal.votes.len() >= min)
                .unwrap_or(true);

            status_match && keyword_match && proposer_match && proposal_type_match && votes_match
        })
        .collect::<Vec<_>>();

    // Sort the proposals based on the sort_by and sort_direction parameters
    if let Some(sort_criteria) = sort_by {
        let is_ascending = sort_direction
            .as_deref()
            .map(|d| d.to_lowercase() == "asc")
            .unwrap_or(true);

        match sort_criteria {
            SortBy::CreationTime => filtered_proposals.sort_by(|a, b| {
                let ordering = a.submission_time.cmp(&b.submission_time);
                if is_ascending {
                    ordering
                } else {
                    ordering.reverse()
                }
            }),
            // Generaly the same as creation time, might be better to delete this
            SortBy::ExpiryTime => filtered_proposals.sort_by(|a, b| {
                let ordering = (a.submission_time.0 + policy.proposal_period.0)
                    .cmp(&(b.submission_time.0 + policy.proposal_period.0));
                if is_ascending {
                    ordering
                } else {
                    ordering.reverse()
                }
            }),
        }
    };

    Json(filtered_proposals)
}

#[get("/proposals/<dao_id>/<proposal_id>")]
async fn get_specific_proposal(
    dao_id: String,
    proposal_id: u64,
    cache: &State<ProposalCache>,
) -> Result<Json<Proposal>, Status> {
    // Check if we have a fresh cache entry for this specific proposal
    let cache_key = (dao_id.clone(), proposal_id);
    {
        let cache_read = cache
            .read()
            .expect("Failed to acquire read lock on proposal cache");

        if let Some(cached) = cache_read.get(&cache_key) {
            if cached.last_updated.elapsed() <= Duration::from_secs(5) {
                return Ok(Json(cached.proposal.clone()));
            }
        };
    }

    // If we didn't return early, we need to fetch the proposal
    let client = rpc_client::get_rpc_client();
    let dao_id_account: AccountId = dao_id.parse().unwrap();

    let proposal = fetch_proposal(&client, &dao_id_account, proposal_id)
        .await
        .unwrap();
    let version = fetch_contract_version(&client, &dao_id_account)
        .await
        .unwrap();

    let mut cache_write = cache
        .write()
        .expect("Failed to acquire write lock on proposal cache");

    cache_write.insert(
        cache_key,
        CachedProposal {
            proposal: proposal.clone(),
            last_updated: Instant::now(),
            version,
        },
    );

    return Ok(Json(proposal));
}

#[launch]
fn rocket() -> _ {
    let proposals_store: ProposalStore = Arc::new(RwLock::new(HashMap::new()));
    let proposal_cache: ProposalCache = Arc::new(RwLock::new(HashMap::new()));

    rocket::build()
        .manage(proposals_store)
        .manage(proposal_cache)
        .mount("/", routes![get_dao_proposals, get_specific_proposal])
        .configure(
            rocket::Config::figment()
                .merge(("port", 5001))
                .merge(("address", "0.0.0.0")),
        )
}
