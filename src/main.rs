#[macro_use]
extern crate rocket;
mod rpc_client;
mod scraper;

use near_primitives::types::AccountId;
use rocket::State;
use rocket::form::{FromForm, FromFormField};
use rocket::http::Status;
use rocket::serde::Serialize;
use rocket::serde::json::Json;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio;

use scraper::{
    Policy, Proposal, ProposalStatus, StateVersion, TxMetadata, fetch_contract_version,
    fetch_policy, fetch_proposal, fetch_proposal_log_txs, fetch_proposals,
};

#[derive(Clone)]
struct CachedProposals {
    proposals: Vec<Proposal>,
    policy: scraper::Policy,
    last_updated: Instant,
    version: StateVersion,
}

#[derive(Serialize, Debug)]
struct ProposalOutput {
    #[serde(flatten)]
    proposal: Proposal,
    txs_log: Vec<TxMetadata>,
}

#[derive(Clone)]
struct CachedProposal {
    proposal: Proposal,
    last_updated: Instant,
    txs_log: Vec<TxMetadata>,
}

type ProposalStore = Arc<RwLock<HashMap<String, CachedProposals>>>;
type ProposalCache = Arc<RwLock<HashMap<(String, u64), CachedProposal>>>;

#[derive(Deserialize, FromFormField)]
enum SortBy {
    CreationTime,
    ExpiryTime,
}

#[derive(Deserialize, FromForm)]
struct ProposalFilters {
    status: Option<ProposalStatus>,
    keyword: Option<String>,
    proposer: Option<String>,
    proposal_type: Option<String>,
    min_votes: Option<usize>,
    sort_by: Option<SortBy>,
    sort_direction: Option<String>,
}

impl ProposalFilters {
    fn filter_proposals(&self, proposals: Vec<Proposal>, policy: &Policy) -> Vec<Proposal> {
        let mut filtered_proposals = proposals
            .into_iter()
            .filter(|proposal| {
                let status_match = self
                    .status
                    .as_ref()
                    .map(|s| proposal.status == *s)
                    .unwrap_or(true);

                let keyword_match = self
                    .keyword
                    .as_ref()
                    .map(|k| {
                        proposal
                            .description
                            .to_lowercase()
                            .contains(&k.to_lowercase())
                    })
                    .unwrap_or(true);

                let proposer_match = self
                    .proposer
                    .as_ref()
                    .map(|p| proposal.proposer == *p)
                    .unwrap_or(true);

                let proposal_type_match = self
                    .proposal_type
                    .as_ref()
                    .map(|pt| {
                        proposal
                            .kind
                            .to_string()
                            .to_lowercase()
                            .contains(&pt.to_lowercase())
                    })
                    .unwrap_or(true);

                let votes_match = self
                    .min_votes
                    .map(|min| proposal.votes.len() >= min)
                    .unwrap_or(true);

                status_match
                    && keyword_match
                    && proposer_match
                    && proposal_type_match
                    && votes_match
            })
            .collect::<Vec<_>>();

        // Sort the proposals based on the sort_by and sort_direction parameters
        if let Some(sort_criteria) = &self.sort_by {
            let is_ascending = self
                .sort_direction
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
                // TODO: probably the same thing, remove?
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

        filtered_proposals
    }
}

async fn get_latest_dao_cache(store: &ProposalStore, dao_id: &AccountId) -> CachedProposals {
    {
        let store_read = store
            .read()
            .expect("Failed to acquire read lock on proposal store");

        if let Some(c) = store_read.get(dao_id.as_str()) {
            // If cache is still valid (not older than 5 seconds), use it
            if c.last_updated.elapsed() <= Duration::from_secs(5) {
                return c.clone();
            }
        }
    };
    let client = rpc_client::get_rpc_client();

    let res = tokio::try_join!(
        fetch_proposals(&client, &dao_id),
        fetch_policy(&client, &dao_id),
        fetch_contract_version(&client, &dao_id)
    );

    // Fetch data in parallel
    let (proposals, policy, version) = res.unwrap();

    // Update the cache
    let mut store_write = store
        .write()
        .expect("Failed to acquire write lock on proposal store");
    let new_cache = CachedProposals {
        proposals,
        policy,
        last_updated: Instant::now(),
        version,
    };
    store_write.insert(dao_id.to_string(), new_cache.clone());
    new_cache
}

async fn get_latest_proposal_cache(
    cache: &ProposalCache,
    dao_id: &AccountId,
    proposal_id: u64,
) -> ProposalOutput {
    let cache_key = (dao_id.to_string(), proposal_id);
    let last_cached_proposal: Option<CachedProposal> = {
        let cache_read = cache
            .read()
            .expect("Failed to acquire read lock on proposal cache");

        if let Some(cached) = cache_read.get(&cache_key) {
            if cached.last_updated.elapsed() <= Duration::from_secs(5) {
                return ProposalOutput {
                    proposal: cached.proposal.clone(),
                    txs_log: cached.txs_log.clone(),
                };
            }
            Some(cached.clone())
        } else {
            None
        }
    };

    // Fetch proposal and version in parallel
    let client = rpc_client::get_rpc_client();
    let block_height_limit = last_cached_proposal
        .as_ref()
        .map_or(0, |c| c.txs_log.last().unwrap().2);
    let (proposal, txs_log) = tokio::try_join!(
        fetch_proposal(&client, &dao_id, proposal_id),
        fetch_proposal_log_txs(&client, dao_id, proposal_id, block_height_limit)
    )
    .unwrap();

    let txs_log =
        last_cached_proposal.map_or(txs_log.clone(), |c| [&c.txs_log[..], &txs_log[..]].concat());

    // Update the cache
    let mut cache_write = cache
        .write()
        .expect("Failed to acquire write lock on proposal cache");

    cache_write.insert(
        cache_key,
        CachedProposal {
            proposal: proposal.clone(),
            last_updated: Instant::now(),
            txs_log: txs_log.clone(),
        },
    );

    ProposalOutput { proposal, txs_log }
}

#[get("/proposals/<dao_id>?<filters..>")]
async fn get_dao_proposals(
    dao_id: String,
    filters: ProposalFilters,
    store: &State<ProposalStore>,
) -> Json<Vec<Proposal>> {
    let dao_id: AccountId = dao_id.parse().unwrap();

    let cached = get_latest_dao_cache(&store, &dao_id).await;
    let filtered_proposals = filters.filter_proposals(cached.proposals, &cached.policy);
    Json(filtered_proposals)
}

#[get("/proposals/<dao_id>/<proposal_id>")]
async fn get_specific_proposal(
    dao_id: String,
    proposal_id: u64,
    cache: &State<ProposalCache>,
) -> Result<Json<ProposalOutput>, Status> {
    let dao_id_account: AccountId = dao_id.parse().unwrap();
    let proposal = get_latest_proposal_cache(cache, &dao_id_account, proposal_id).await;
    Ok(Json(proposal))
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
