use crate::cache::{FtMetadataCache, get_ft_metadata_cache};
use crate::scraper::{
    AssetExchangeInfo, LockupInfo, PaymentInfo, Policy, Proposal, ProposalType,
    StakeDelegationInfo, get_status_display,
};

use near_jsonrpc_client::JsonRpcClient;
use rocket::form::{FromForm, FromFormField};
use rocket::serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;

// Helper function to convert human-readable amount to smallest unit
fn convert_to_smallest_unit(amount: &str, decimals: u8) -> Option<u128> {
    amount
        .parse::<f64>()
        .ok()
        .map(|v| (v * 10f64.powi(decimals as i32)) as u128)
}

// Helper function to parse date string "2024-09-10" to timestamp
fn parse_date_to_timestamp(date_str: &str) -> Result<u64, Box<dyn std::error::Error>> {
    use chrono::{NaiveDate, TimeZone, Utc};

    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
    let datetime = date.and_hms_opt(0, 0, 0).unwrap();
    let utc_datetime = Utc.from_utc_datetime(&datetime);

    // Convert to nanoseconds (same format as proposal timestamps)
    Ok(utc_datetime.timestamp_nanos_opt().unwrap_or(0) as u64)
}

#[derive(Deserialize, FromFormField, Clone)]
pub enum SortBy {
    CreationTime,
    ExpiryTime,
}

pub mod categories {
    pub const PAYMENTS: &str = "payments";
    pub const LOCKUP: &str = "lockup";
    pub const ASSET_EXCHANGE: &str = "asset-exchange";
    pub const STAKE_DELEGATION: &str = "stake-delegation";
}

#[derive(Deserialize, FromForm, Default, Clone)]
pub struct ProposalFilters {
    pub statuses: Option<String>, // comma-separated values like "Approved,Rejected"
    pub search: Option<String>,   // search the description
    pub proposal_types: Option<String>, // comma-separated values like 'FunctionCall,Transfer'
    pub sort_by: Option<SortBy>,
    pub sort_direction: Option<String>, // "asc" or "desc"
    pub category: Option<String>,
    pub created_date_from: Option<String>,
    pub created_date_to: Option<String>,

    pub proposers: Option<String>,     // comma-separated accounts
    pub proposers_not: Option<String>, // comma-separated accounts

    pub approvers: Option<String>,     // comma-separated accounts
    pub approvers_not: Option<String>, // array of accounts
    pub voter_votes: Option<String>, // format: "account:vote,account:vote" where vote is "approved" or "rejected"
    // Payment-specific filters
    pub recipients: Option<String>,     // comma-separated accounts
    pub recipients_not: Option<String>, // comma-separated accounts
    pub tokens: Option<String>,         // comma-separated ft token ids
    pub tokens_not: Option<String>,     // comma-separated ft token ids
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub amount_equal: Option<String>,
    // Pagination
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

fn to_str_hashset(opt: &Option<String>) -> Option<HashSet<&str>> {
    opt.as_ref()
        .map(|s| s.split(',').map(|s| s.trim()).collect())
}

#[derive(Debug, Clone)]
struct VoterVote {
    account: String,
    expected_vote: String,
}

fn parse_voter_votes(opt: &Option<String>) -> Option<Vec<VoterVote>> {
    opt.as_ref().map(|s| {
        s.split(',')
            .filter_map(|pair| {
                let parts: Vec<&str> = pair.trim().split(':').collect();
                if parts.len() == 2 {
                    Some(VoterVote {
                        account: parts[0].trim().to_string(),
                        expected_vote: parts[1].trim().to_lowercase(),
                    })
                } else {
                    None
                }
            })
            .collect()
    })
}

impl ProposalFilters {
    pub async fn filter_proposals_async(
        &self,
        proposals: Vec<Proposal>,
        policy: &Policy,
        ft_metadata_cache: &FtMetadataCache,
    ) -> Result<Vec<Proposal>, Box<dyn std::error::Error>> {
        let client = Arc::new(JsonRpcClient::connect("https://rpc.mainnet.near.org"));

        let statuses_set = to_str_hashset(&self.statuses);
        let proposers_set = to_str_hashset(&self.proposers);
        let proposers_not_set = to_str_hashset(&self.proposers_not);
        let approvers_set = to_str_hashset(&self.approvers);
        let approvers_not_set = to_str_hashset(&self.approvers_not);
        let voter_votes_set = parse_voter_votes(&self.voter_votes);
        let recipients_set = to_str_hashset(&self.recipients);
        let recipients_not_set = to_str_hashset(&self.recipients_not);
        let tokens_set = to_str_hashset(&self.tokens);
        let tokens_not_set = to_str_hashset(&self.tokens_not);
        let proposal_types_set = to_str_hashset(&self.proposal_types);

        let search_keywords: Option<Vec<String>> = self.search.as_ref().map(|s| {
            s.split(',')
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .collect()
        });

        let from_timestamp = self
            .created_date_from
            .as_ref()
            .and_then(|d| parse_date_to_timestamp(d).ok());
        let to_timestamp = self
            .created_date_to
            .as_ref()
            .and_then(|d| parse_date_to_timestamp(d).ok());

        let mut filtered_proposals = Vec::with_capacity(proposals.len());

        for proposal in proposals {
            let submission_time = proposal.submission_time.0;

            if let Some(ref proposers) = proposers_set {
                if !proposers.contains(proposal.proposer.as_str()) {
                    continue;
                }
            }

            if let Some(ref proposers_not) = proposers_not_set {
                if proposers_not.contains(proposal.proposer.as_str()) {
                    continue;
                }
            }

            if let Some(ref approvers) = approvers_set {
                let has_any_approver = approvers
                    .iter()
                    .any(|approver| proposal.votes.contains_key(*approver));
                if !has_any_approver {
                    continue;
                }
            }

            if let Some(ref approvers_not) = approvers_not_set {
                let has_any_excluded_approver = approvers_not
                    .iter()
                    .any(|approver| proposal.votes.contains_key(*approver));
                if has_any_excluded_approver {
                    continue;
                }
            }

            if let Some(from_ts) = from_timestamp {
                if submission_time < from_ts {
                    continue;
                }
            }
            if let Some(to_ts) = to_timestamp {
                if submission_time > to_ts {
                    continue;
                }
            }

            if let Some(ref statuses) = statuses_set {
                let computed_status = get_status_display(
                    &proposal.status,
                    submission_time,
                    policy.proposal_period.0,
                    "InProgress",
                );
                if !statuses.contains(computed_status.as_str()) {
                    continue;
                }
            }

            if let Some(ref keywords) = search_keywords {
                let proposal_id_str = proposal.id.to_string();

                if !keywords.iter().any(|kw| {
                    // If keyword is only numbers, search for exact proposal ID match
                    if kw.chars().all(|c| c.is_ascii_digit()) {
                        proposal_id_str == *kw
                    } else {
                        // Otherwise search in both description and proposal ID
                        proposal.description.to_lowercase().contains(kw)
                            || proposal_id_str.to_lowercase().contains(kw)
                    }
                }) {
                    continue;
                }
            }

            if let Some(ref proposal_types) = proposal_types_set {
                let proposal_kind_keys: Vec<&str> = proposal
                    .kind
                    .as_object()
                    .map(|obj| obj.keys().map(|k| k.as_str()).collect())
                    .unwrap_or_default();

                if !proposal_types
                    .iter()
                    .any(|proposal_type| proposal_kind_keys.contains(proposal_type))
                {
                    continue;
                }
            }

            if let Some(ref voter_votes) = voter_votes_set {
                let mut all_voter_checks_passed = true;
                for voter_vote in voter_votes {
                    let actual_vote = proposal.votes.get(&voter_vote.account);
                    let vote_status = match actual_vote {
                        Some(crate::scraper::Vote::Approve) => "approved",
                        Some(crate::scraper::Vote::Reject) | Some(crate::scraper::Vote::Remove) => {
                            "rejected"
                        }
                        None => {
                            // If voter didn't vote, this proposal doesn't match
                            all_voter_checks_passed = false;
                            break;
                        }
                    };

                    if vote_status != voter_vote.expected_vote {
                        all_voter_checks_passed = false;
                        break;
                    }
                }

                if !all_voter_checks_passed {
                    continue;
                }
            }

            if let Some(category) = &self.category {
                match category.as_str() {
                    categories::LOCKUP => {
                        if LockupInfo::from_proposal(&proposal).is_none() {
                            continue;
                        }
                    }
                    categories::ASSET_EXCHANGE => {
                        if AssetExchangeInfo::from_proposal(&proposal).is_none() {
                            continue;
                        }
                    }
                    categories::STAKE_DELEGATION => {
                        if StakeDelegationInfo::from_proposal(&proposal).is_none() {
                            continue;
                        }
                    }
                    categories::PAYMENTS => {
                        if let Some(payment_info) = PaymentInfo::from_proposal(&proposal) {
                            let token_to_check = if payment_info.token.is_empty() {
                                "near"
                            } else {
                                payment_info.token.as_str()
                            };

                            if let Some(ref recipients) = recipients_set {
                                if !recipients.contains(payment_info.receiver.as_str()) {
                                    continue;
                                }
                            }

                            if let Some(ref recipients_not) = recipients_not_set {
                                if recipients_not.contains(payment_info.receiver.as_str()) {
                                    continue;
                                }
                            }

                            if let Some(ref tokens) = tokens_set {
                                if !tokens.contains(token_to_check) {
                                    continue;
                                }
                            }

                            if let Some(ref tokens_not) = tokens_not_set {
                                if tokens_not.contains(token_to_check) {
                                    continue;
                                }
                            }

                            if self.amount_equal.is_some()
                                || self.amount_min.is_some()
                                || self.amount_max.is_some()
                            {
                                // Get token metadata for amount comparison
                                let token_id = if payment_info.token.is_empty() {
                                    "near"
                                } else {
                                    &payment_info.token
                                };

                                let ft_metadata =
                                    get_ft_metadata_cache(&client, ft_metadata_cache, token_id)
                                        .await?;
                                let token_decimals = ft_metadata.decimals;

                                let proposal_amount = payment_info.amount.parse::<u128>().ok();

                                if let Some(amount_equal_str) = &self.amount_equal {
                                    if let Some(amount_equal) =
                                        convert_to_smallest_unit(amount_equal_str, token_decimals)
                                    {
                                        if let Some(amount) = proposal_amount {
                                            if amount != amount_equal {
                                                continue;
                                            }
                                        } else {
                                            continue; // Invalid amount
                                        }
                                    } else {
                                        continue; // Invalid amount_equal input
                                    }
                                }

                                if let Some(min_str) = &self.amount_min {
                                    if let Some(min) =
                                        convert_to_smallest_unit(min_str, token_decimals)
                                    {
                                        if let Some(amount) = proposal_amount {
                                            if amount < min {
                                                continue;
                                            }
                                        } else {
                                            continue; // Invalid amount
                                        }
                                    } else {
                                        continue; // Invalid amount_min input
                                    }
                                }

                                if let Some(max_str) = &self.amount_max {
                                    if let Some(max) =
                                        convert_to_smallest_unit(max_str, token_decimals)
                                    {
                                        if let Some(amount) = proposal_amount {
                                            if amount > max {
                                                continue;
                                            }
                                        } else {
                                            continue; // Invalid amount
                                        }
                                    } else {
                                        continue; // Invalid amount_max input
                                    }
                                }
                            } // Close the amount filters conditional block
                        } else {
                            continue; // Not a payment proposal
                        }
                    }
                    _ => {}
                }
            }

            filtered_proposals.push(proposal);
        }

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
        }

        Ok(filtered_proposals)
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
