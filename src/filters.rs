use crate::scraper::{CountsVersions, Policy, Proposal, ProposalStatus};
use anyhow::{Result, anyhow};
use near_sdk::AccountId;
use rocket::form::{FromForm, FromFormField};
use rocket::serde::Deserialize;

#[derive(Deserialize, FromFormField)]
enum SortBy {
    CreationTime,
    ExpiryTime,
}

#[derive(Deserialize, FromForm)]
pub struct ProposalFilters {
    status: Option<ProposalStatus>,
    keyword: Option<String>,
    proposer: Option<String>,
    proposal_type: Option<Vec<String>>,
    min_votes: Option<usize>,
    approvers: Option<Vec<String>>,
    sort_by: Option<SortBy>,
    sort_direction: Option<String>,
}

impl ProposalFilters {
    pub fn filter_proposals(&self, proposals: Vec<Proposal>, policy: &Policy) -> Vec<Proposal> {
        let mut filtered_proposals = proposals
            .into_iter()
            .filter(|proposal| {
                if let Some(status) = &self.status {
                    if proposal.status != *status {
                        return false;
                    }
                }

                if let Some(keyword) = &self.keyword {
                    if proposal
                        .description
                        .to_lowercase()
                        .contains(&keyword.to_lowercase())
                        != true
                    {
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

                if let Some(types) = &self.proposal_type {
                    if types
                        .iter()
                        .all(|pt| filter_proposal_type(pt, proposal).unwrap_or(false))
                        != true
                    {
                        return false;
                    }
                }

                if let Some(min) = self.min_votes {
                    if proposal.votes.len() < min {
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
        println!("c: {}, v: {}", command, value);
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
