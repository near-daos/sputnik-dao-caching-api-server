use crate::scraper::{
    AssetExchangeInfo, LockupInfo, PaymentInfo, Policy, Proposal, ProposalType,
    StakeDelegationInfo, get_status_display,
};

use rocket::form::{FromForm, FromFormField};
use rocket::serde::Deserialize;
use std::collections::HashSet;

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
    pub created_between: Option<String>,

    pub proposers: Option<String>,     // comma-separated accounts
    pub proposers_not: Option<String>, // comma-separated accounts

    pub approvers: Option<String>,     // comma-separated accounts
    pub approvers_not: Option<String>, // array of accounts
    // Payment-specific filters
    pub recipients: Option<String>,     // comma-separated accounts
    pub recipients_not: Option<String>, // comma-separated accounts
    pub tokens: Option<String>,         // comma-separated ft token ids
    pub tokens_not: Option<String>,     // comma-separated ft token ids
    pub amount_min: Option<u128>,
    pub amount_max: Option<u128>,
    // Pagination
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

fn to_str_hashset(opt: &Option<String>) -> Option<HashSet<&str>> {
    opt.as_ref()
        .map(|s| s.split(',').map(|s| s.trim()).collect())
}

impl ProposalFilters {
    pub fn filter_proposals(&self, proposals: Vec<Proposal>, policy: &Policy) -> Vec<Proposal> {
        let statuses_set = to_str_hashset(&self.statuses);
        let proposers_set = to_str_hashset(&self.proposers);
        let proposers_not_set = to_str_hashset(&self.proposers_not);
        let approvers_set = to_str_hashset(&self.approvers);
        let approvers_not_set = to_str_hashset(&self.approvers_not);
        let recipients_set = to_str_hashset(&self.recipients);
        let recipients_not_set = to_str_hashset(&self.recipients_not);
        let tokens_set = to_str_hashset(&self.tokens);
        let tokens_not_set = to_str_hashset(&self.tokens_not);

        let search_keywords: Option<Vec<String>> = self.search.as_ref().map(|s| {
            s.split(',')
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .collect()
        });

        let mut filtered_proposals = proposals
            .into_iter()
            .filter(|proposal| {
                if let Some(ref statuses) = statuses_set {
                    let computed_status = get_status_display(
                        &proposal.status,
                        proposal.submission_time.0,
                        policy.proposal_period.0,
                        "InProgress",
                    );
                    if !statuses.contains(computed_status.as_str()) {
                        return false;
                    }
                }

                if let Some(ref proposers) = proposers_set {
                    if !proposers.contains(proposal.proposer.as_str()) {
                        return false;
                    }
                }

                if let Some(ref proposers_not) = proposers_not_set {
                    if proposers_not.contains(proposal.proposer.as_str()) {
                        return false;
                    }
                }

                if let Some(ref keywords) = search_keywords {
                    let description_lower = proposal.description.to_lowercase();
                    if !keywords.iter().any(|kw| description_lower.contains(kw)) {
                        return false;
                    }
                }

                if let Some(ref created_between) = self.created_between {
                    let parts: Vec<&str> = created_between.split(',').collect();
                    if parts.len() == 2 {
                        if let (Ok(start), Ok(end)) = (
                            parts[0].trim().parse::<u64>(),
                            parts[1].trim().parse::<u64>(),
                        ) {
                            let submission_time = proposal.submission_time.0;
                            if submission_time < start || submission_time > end {
                                return false;
                            }
                        }
                    }
                }

                if let Some(ref approvers) = approvers_set {
                    // Check if ANY of the specified approvers have voted (OR logic)
                    let has_any_approver = approvers
                        .iter()
                        .any(|approver| proposal.votes.contains_key(*approver));
                    if !has_any_approver {
                        return false;
                    }
                }

                if let Some(ref approvers_not) = approvers_not_set {
                    // Check if ANY of the specified approvers have voted (exclude if any match)
                    let has_any_excluded_approver = approvers_not
                        .iter()
                        .any(|approver| proposal.votes.contains_key(*approver));
                    if has_any_excluded_approver {
                        return false;
                    }
                }

                if let Some(category) = &self.category {
                    match category.as_str() {
                        categories::LOCKUP => {
                            if LockupInfo::from_proposal(proposal).is_none() {
                                return false;
                            }
                        }
                        categories::ASSET_EXCHANGE => {
                            if AssetExchangeInfo::from_proposal(proposal).is_none() {
                                return false;
                            }
                        }
                        categories::STAKE_DELEGATION => {
                            if StakeDelegationInfo::from_proposal(proposal).is_none() {
                                return false;
                            }
                        }
                        categories::PAYMENTS => {
                            // Handle payment filters
                            if let Some(payment_info) = PaymentInfo::from_proposal(proposal) {
                                if let Some(ref recipients) = recipients_set {
                                    if !recipients.contains(payment_info.receiver.as_str()) {
                                        return false;
                                    }
                                }

                                if let Some(ref recipients_not) = recipients_not_set {
                                    if recipients_not.contains(payment_info.receiver.as_str()) {
                                        return false;
                                    }
                                }

                                if let Some(ref tokens) = tokens_set {
                                    // Handle empty string in proposal as NEAR token
                                    let token_to_check = if payment_info.token.is_empty() {
                                        "near"
                                    } else {
                                        payment_info.token.as_str()
                                    };
                                    if !tokens.contains(token_to_check) {
                                        return false;
                                    }
                                }

                                if let Some(ref tokens_not) = tokens_not_set {
                                    // Handle empty string in proposal as NEAR token
                                    let token_to_check = if payment_info.token.is_empty() {
                                        "near"
                                    } else {
                                        payment_info.token.as_str()
                                    };
                                    if tokens_not.contains(token_to_check) {
                                        return false;
                                    }
                                }

                                if let Some(min) = self.amount_min {
                                    if let Ok(amount) = payment_info.amount.parse::<u128>() {
                                        if amount < min {
                                            return false;
                                        }
                                    } else {
                                        return false; // Invalid amount
                                    }
                                }

                                if let Some(max) = self.amount_max {
                                    if let Ok(amount) = payment_info.amount.parse::<u128>() {
                                        if amount > max {
                                            return false;
                                        }
                                    } else {
                                        return false; // Invalid amount
                                    }
                                }
                            } else {
                                return false; // Not a payment proposal
                            }
                        }
                        _ => {}
                    }
                }

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
