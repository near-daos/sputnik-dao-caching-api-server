use crate::scraper::{Proposal, ProposalStatus};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProposalCsvView {
    pub id: u64,
    pub proposer: String,
    pub description: String,
    pub kind: String,
    pub status: ProposalStatus,
    pub vote_counts: String,
    pub votes: String,
    pub submission_time: u64,
    pub last_actions_log: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txs_log: Option<String>,
}

impl From<Proposal> for ProposalCsvView {
    fn from(proposal: Proposal) -> Self {
        Self {
            id: proposal.id,
            proposer: proposal.proposer,
            description: proposal.description,
            kind: proposal.kind.to_string(),
            status: proposal.status,
            vote_counts: serde_json::to_string(&proposal.vote_counts).unwrap_or_default(),
            votes: serde_json::to_string(&proposal.votes).unwrap_or_default(),
            submission_time: proposal.submission_time.0,
            last_actions_log: serde_json::to_string(&proposal.last_actions_log).unwrap_or_default(),
            txs_log: None,
        }
    }
}
