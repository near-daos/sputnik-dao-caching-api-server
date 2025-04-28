use crate::scraper::{Policy, Proposal, ProposalStatus};
use anyhow::{Result, anyhow};
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
    sort_by: Option<SortBy>,
    sort_direction: Option<String>,
}

impl ProposalFilters {
    pub fn filter_proposals(&self, proposals: Vec<Proposal>, policy: &Policy) -> Vec<Proposal> {
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
                    .map(|types| {
                        types
                            .iter()
                            .all(|pt| filter_proposal_type(pt, proposal).unwrap_or(false))
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
