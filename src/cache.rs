use anyhow::Result;
use borsh::{BorshDeserialize, BorshSerialize};
use near_jsonrpc_client::JsonRpcClient;
use near_primitives::types::AccountId;
use near_sdk::json_types::U64;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio;

use crate::scraper::{
    FtMetadata, Policy, Proposal, ProposalStatus, StateVersion, TxMetadata, fetch_contract_version,
    fetch_ft_metadata, fetch_policy, fetch_proposal, fetch_proposal_log_txs, fetch_proposals,
};

const CACHE_LIFE_TIME: Duration = Duration::from_secs(5);
const FT_CACHE_LIFETIME: Duration = Duration::from_secs(60 * 60); // 60 minutes

#[derive(Clone, Debug)]
pub struct CachedProposals {
    pub proposals: Vec<Proposal>,
    pub policy: Policy,
    pub last_updated: Instant,
    pub version: StateVersion,
}

#[derive(Clone, BorshSerialize)]
pub struct CachedProposal {
    #[borsh(skip)]
    pub proposal: Proposal,
    #[borsh(skip)]
    pub last_updated: Instant,
    pub txs_log: Vec<TxMetadata>,
}

pub struct CachedFtMetadata {
    pub metadata: FtMetadata,
    pub last_updated: Instant,
}

pub type FtMetadataCache = Arc<RwLock<HashMap<AccountId, CachedFtMetadata>>>;

// Required to store in storage
impl BorshDeserialize for CachedProposal {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let txs_log = Vec::<TxMetadata>::deserialize_reader(reader)?;

        // Create the struct with default values for skipped fields
        Ok(CachedProposal {
            proposal: Proposal {
                id: 0,
                proposer: "".parse().unwrap(),
                description: "".to_string(),
                kind: Value::default(),
                status: ProposalStatus::InProgress,
                vote_counts: HashMap::new(),
                votes: HashMap::new(),
                submission_time: U64(0),
                last_actions_log: None,
            },
            last_updated: Instant::now() - CACHE_LIFE_TIME,
            txs_log,
        })
    }
}

pub type ProposalStore = Arc<RwLock<HashMap<String, CachedProposals>>>;
pub type ProposalCache = Arc<RwLock<HashMap<(String, u64), CachedProposal>>>;

pub async fn get_latest_dao_cache(
    client: &Arc<JsonRpcClient>,
    store: &ProposalStore,
    dao_id: &AccountId,
) -> Result<CachedProposals> {
    {
        let store_read = store
            .read()
            .expect("Failed to acquire read lock on proposal store");

        if let Some(c) = store_read.get(dao_id.as_str()) {
            if c.last_updated.elapsed() <= CACHE_LIFE_TIME {
                println!("Cache hit for DAO ID: {}", dao_id);
                return Ok(c.clone());
            }
        }
    }

    let (proposals, policy, version) = tokio::try_join!(
        fetch_proposals(&client, &dao_id),
        fetch_policy(&client, &dao_id),
        fetch_contract_version(&client, &dao_id)
    )?;

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
    Ok(new_cache)
}

pub async fn get_latest_proposal_cache(
    client: &Arc<JsonRpcClient>,
    cache: &ProposalCache,
    dao_id: &AccountId,
    proposal_id: u64,
) -> Result<CachedProposal> {
    let cache_key = (dao_id.to_string(), proposal_id);
    let last_cached_proposal: Option<CachedProposal> = {
        let cache_read = cache
            .read()
            .expect("Failed to acquire read lock on proposal cache");

        if let Some(cached) = cache_read.get(&cache_key) {
            if cached.last_updated.elapsed() <= CACHE_LIFE_TIME {
                return Ok(cached.clone());
            }
            Some(cached.clone())
        } else {
            None
        }
    };

    // Fetch proposal and version in parallel
    let block_height_limit = last_cached_proposal
        .as_ref()
        .map_or(0, |c| c.txs_log.last().map(|l| l.block_height).unwrap_or(0));
    let (proposal, txs_log) = tokio::try_join!(
        fetch_proposal(&client, &dao_id, proposal_id),
        fetch_proposal_log_txs(&client, dao_id, proposal_id, block_height_limit)
    )?;

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

    Ok(cached_proposal)
}

pub async fn get_ft_metadata_cache(
    client: &Arc<JsonRpcClient>,
    cache: &FtMetadataCache,
    contract_id: &str,
) -> Result<FtMetadata> {
    // Check if token is empty or NEAR (case-insensitive)
    if contract_id.is_empty() || contract_id.eq_ignore_ascii_case("near") {
        return Ok(FtMetadata::near());
    }

    let token_id = contract_id.parse::<AccountId>()?;

    // Acquire read lock and check cache
    {
        let cache_read = match cache.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: cache read lock poisoned, recovering.");
                poisoned.into_inner()
            }
        };

        if let Some(cached) = cache_read.get(&token_id) {
            if cached.last_updated.elapsed() <= FT_CACHE_LIFETIME {
                return Ok(cached.metadata.clone());
            }
        }
    }

    let metadata = fetch_ft_metadata(client, &token_id).await?;

    // Acquire write lock to update cache
    let mut cache_write = match cache.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("Warning: cache write lock poisoned, recovering.");
            poisoned.into_inner()
        }
    };

    cache_write.insert(
        token_id.clone(),
        CachedFtMetadata {
            metadata: metadata.clone(),
            last_updated: Instant::now(),
        },
    );
    Ok(metadata)
}
