use near_jsonrpc_client::JsonRpcClient;
use near_primitives::types::AccountId;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio;

use crate::scraper::{
    Policy, Proposal, StateVersion, TxMetadata, fetch_contract_version, fetch_policy,
    fetch_proposal, fetch_proposal_log_txs, fetch_proposals,
};

const CACHE_LIFE_TIME: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct CachedProposals {
    pub proposals: Vec<Proposal>,
    pub policy: Policy,
    pub last_updated: Instant,
    pub version: StateVersion,
}

#[derive(Clone)]
pub struct CachedProposal {
    pub proposal: Proposal,
    pub last_updated: Instant,
    pub txs_log: Vec<TxMetadata>,
}

pub type ProposalStore = Arc<RwLock<HashMap<String, CachedProposals>>>;
pub type ProposalCache = Arc<RwLock<HashMap<(String, u64), CachedProposal>>>;

pub async fn get_latest_dao_cache(
    client: &Arc<JsonRpcClient>,
    store: &ProposalStore,
    dao_id: &AccountId,
) -> CachedProposals {
    {
        let store_read = store
            .read()
            .expect("Failed to acquire read lock on proposal store");

        if let Some(c) = store_read.get(dao_id.as_str()) {
            if c.last_updated.elapsed() <= CACHE_LIFE_TIME {
                return c.clone();
            }
        }
    };

    let (proposals, policy, version) = tokio::try_join!(
        fetch_proposals(&client, &dao_id),
        fetch_policy(&client, &dao_id),
        fetch_contract_version(&client, &dao_id)
    )
    .unwrap();

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

pub async fn get_latest_proposal_cache(
    client: &Arc<JsonRpcClient>,
    cache: &ProposalCache,
    dao_id: &AccountId,
    proposal_id: u64,
) -> CachedProposal {
    let cache_key = (dao_id.to_string(), proposal_id);
    let last_cached_proposal: Option<CachedProposal> = {
        let cache_read = cache
            .read()
            .expect("Failed to acquire read lock on proposal cache");

        if let Some(cached) = cache_read.get(&cache_key) {
            if cached.last_updated.elapsed() <= CACHE_LIFE_TIME {
                return cached.clone();
            }
            Some(cached.clone())
        } else {
            None
        }
    };

    // Fetch proposal and version in parallel
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

    let cached_proposal = CachedProposal {
        proposal: proposal.clone(),
        last_updated: Instant::now(),
        txs_log: txs_log.clone(),
    };
    cache_write.insert(cache_key, cached_proposal.clone());

    cached_proposal
}
