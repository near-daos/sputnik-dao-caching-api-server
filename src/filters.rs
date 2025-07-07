use crate::scraper::{
    AssetExchangeInfo, CountsVersions, LockupInfo, PaymentInfo, Policy, Proposal, ProposalStatus,
    ProposalType, StakeDelegationInfo, get_status_display,
};
use anyhow::{Result, anyhow};
use rocket::form::{FromForm, FromFormField};
use rocket::serde::Deserialize;

#[derive(Deserialize, FromFormField)]
enum SortBy {
    CreationTime,
    ExpiryTime,
}

pub mod categories {
    pub const PAYMENTS: &str = "payments";
    pub const LOCKUP: &str = "lockup";
    pub const ASSET_EXCHANGE: &str = "asset-exchange";
    pub const STAKE_DELEGATION: &str = "stake-delegation";
}

#[derive(Deserialize, FromForm, Default)]
pub struct ProposalFilters {
    pub status: Option<String>,
    pub keyword: Option<String>,
    proposer: Option<String>,
    pub proposal_type: Option<String>,
    min_votes: Option<usize>,
    approvers: Option<Vec<String>>,
    sort_by: Option<SortBy>,
    sort_direction: Option<String>,
    pub category: Option<String>,
    // Pagination
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

impl ProposalFilters {
    pub fn filter_proposals(&self, proposals: Vec<Proposal>, policy: &Policy) -> Vec<Proposal> {
        let mut filtered_proposals = proposals
            .into_iter()
            .filter(|proposal| {
                if let Some(status_str) = &self.status {
                    let statuses: Vec<&str> = status_str
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let computed_status = get_status_display(
                        &proposal.status,
                        proposal.submission_time.0,
                        policy.proposal_period.0,
                        "InProgress",
                    );
                    let matched = statuses.iter().any(|status| computed_status == *status);
                    if !matched {
                        return false;
                    }
                }
                if let Some(types_str) = &self.proposal_type {
                    let types: Vec<&str> = types_str
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let matched = types
                        .iter()
                        .any(|pt| filter_proposal_type(pt, proposal).unwrap_or(false));
                    if !matched {
                        return false;
                    }
                }

                if let Some(keyword_str) = &self.keyword {
                    let keywords: Vec<String> = keyword_str
                        .split(',')
                        .map(|k| k.trim().to_lowercase())
                        .filter(|k| !k.is_empty())
                        .collect();

                    let description_lower = proposal.description.to_lowercase();

                    // Return false if *none* of the keywords are found in description
                    if !keywords.iter().any(|kw| description_lower.contains(kw)) {
                        return false;
                    }
                }

                if let Some(proposer) = &self.proposer {
                    if proposal.proposer != *proposer {
                        return false;
                    }
                }

                // All approvers should be present
                if let Some(approvers) = &self.approvers {
                    if !approvers.iter().all(|approver| {
                        proposal
                            .vote_counts
                            .get(approver)
                            .map(|votes| match votes[0] {
                                CountsVersions::V1(v) => v > 0,
                                CountsVersions::V2(v) => v.0 > 0,
                            })
                            .unwrap_or(false)
                    }) {
                        return false;
                    }
                }

                if let Some(min) = self.min_votes {
                    if proposal.votes.len() < min {
                        return false;
                    }
                }

                if let Some(category) = self.category.as_deref() {
                    if category == categories::PAYMENTS
                        && PaymentInfo::from_proposal(proposal).is_none()
                    {
                        return false;
                    }
                    if category == categories::LOCKUP
                        && LockupInfo::from_proposal(proposal).is_none()
                    {
                        return false;
                    }
                    if category == categories::ASSET_EXCHANGE
                        && AssetExchangeInfo::from_proposal(proposal).is_none()
                    {
                        return false;
                    }
                    if category == categories::STAKE_DELEGATION
                        && StakeDelegationInfo::from_proposal(proposal).is_none()
                    {
                        return false;
                    }
                }

                // If we reach here, all filters have passed
                true
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

    pub fn filter_and_extract<T: ProposalType>(
        &self,
        proposals: Vec<Proposal>,
    ) -> Vec<(Proposal, T)> {
        proposals
            .into_iter()
            .filter_map(|proposal| T::from_proposal(&proposal).map(|info| (proposal, info)))
            .collect()
    }
}

fn filter_proposal_type(proposal_type: &str, proposal: &Proposal) -> Result<bool> {
    let parts: Vec<&str> = proposal_type.split(":").collect();
    let mut json_path = proposal.kind.clone();
    let mut last_index = 0;
    for (i, p) in parts.iter().enumerate() {
        if p.starts_with(&['=', '>', '<']) {
            break;
        }
        last_index = i;
        json_path = json_path
            .get_mut(p)
            .ok_or_else(|| anyhow!("JSON path not found"))?
            .take();
    }
    if last_index != parts.len() - 1 {
        let command = &parts[last_index + 1][0..1];
        let value = &parts[last_index + 1][1..];
        match command {
            "=" => Ok(json_path.eq(value)),
            "<" => Ok(json_path
                .as_str()
                .ok_or_else(|| anyhow!("can't convert to str"))?
                .parse::<u128>()?
                .gt(&value.parse::<u128>()?)),
            ">" => Ok(json_path
                .as_str()
                .ok_or_else(|| anyhow!("can't convert to str"))?
                .parse::<u128>()?
                .lt(&value.parse::<u128>()?)),
            _ => {
                // Should not be possible
                Ok(true)
            }
        }
    } else {
        Ok(true)
    }
}
