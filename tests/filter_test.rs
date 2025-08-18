use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rocket::{http::Status, local::asynchronous::Client};
// Empty vec fallback
static EMPTY_VEC: Vec<serde_json::Value> = Vec::new();

const TEST_DAO_ID: &str = "testing-astradao.sputnik-dao.near";

async fn get_test_client() -> Client {
    let rocket = sputnik_indexer::rocket();
    Client::tracked(rocket)
        .await
        .expect("valid rocket instance")
}

#[derive(Debug)]
struct PaymentInfo {
    receiver: String,
    token: String,
    amount: String,
}

fn extract_payment_info(proposal: &serde_json::Value) -> Option<PaymentInfo> {
    let kind = proposal.get("kind")?;

    // Check Transfer kind
    if let Some(transfer) = kind.get("Transfer") {
        let receiver = transfer
            .get("receiver_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let token = transfer
            .get("token_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let amount = transfer
            .get("amount")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        return Some(PaymentInfo {
            receiver,
            token,
            amount,
        });
    }

    // Check FunctionCall kind
    if let Some(function_call) = kind.get("FunctionCall") {
        let receiver_id = function_call
            .get("receiver_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let actions = function_call
            .get("actions")
            .and_then(|a| a.as_array())
            .unwrap_or(&EMPTY_VEC);

        // Check for ft_transfer method
        for action in actions {
            if let Some(method_name) = action.get("method_name").and_then(|m| m.as_str()) {
                if method_name == "ft_transfer" {
                    if let Some(args_b64) = action.get("args").and_then(|a| a.as_str()) {
                        if let Ok(decoded_bytes) = STANDARD.decode(args_b64) {
                            if let Ok(json_args) =
                                serde_json::from_slice::<serde_json::Value>(&decoded_bytes)
                            {
                                let receiver = json_args
                                    .get("receiver_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let amount = json_args
                                    .get("amount")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                // For ft_transfer, the token is the receiver_id
                                let token = receiver_id.to_string();

                                return Some(PaymentInfo {
                                    receiver,
                                    token,
                                    amount,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Check for ft_withdraw method (Intents payments)
        if receiver_id == "intents.near" {
            for action in actions {
                if let Some(method_name) = action.get("method_name").and_then(|m| m.as_str()) {
                    if method_name == "ft_withdraw" {
                        if let Some(args_b64) = action.get("args").and_then(|a| a.as_str()) {
                            if let Ok(decoded_bytes) = STANDARD.decode(args_b64) {
                                if let Ok(json_args) =
                                    serde_json::from_slice::<serde_json::Value>(&decoded_bytes)
                                {
                                    let token = json_args
                                        .get("token")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let amount = json_args
                                        .get("amount")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let receiver = if let Some(memo) =
                                        json_args.get("memo").and_then(|v| v.as_str())
                                    {
                                        if memo.contains("WITHDRAW_TO:") {
                                            memo.split("WITHDRAW_TO:")
                                                .nth(1)
                                                .unwrap_or("")
                                                .to_string()
                                        } else {
                                            json_args
                                                .get("receiver_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .to_string()
                                        }
                                    } else {
                                        json_args
                                            .get("receiver_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string()
                                    };

                                    return Some(PaymentInfo {
                                        receiver,
                                        token,
                                        amount,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

// Helper function to make a request and parse the response
async fn make_request_and_parse(client: &Client, url: &str) -> serde_json::Value {
    let response = client.get(url).dispatch().await;
    assert_eq!(response.status(), Status::Ok, "Request should succeed");

    let response_body = response.into_string().await.unwrap();
    serde_json::from_str(&response_body).expect("Response should be valid JSON")
}

// Helper function to get proposals array from response
fn get_proposals_array(response: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    response.get("proposals").and_then(|p| p.as_array())
}

// Helper function to normalize token for comparison
fn normalize_token(token: &str) -> &str {
    if token.is_empty() { "near" } else { token }
}

// Helper function to validate payment amount with u128 parsing
fn verify_payment_amount(
    proposal: &serde_json::Value,
    min_amount: Option<u128>,
    max_amount: Option<u128>,
    exact_amount: Option<u128>,
) {
    if let Some(payment_info) = extract_payment_info(proposal) {
        if let Ok(amount_u128) = payment_info.amount.parse::<u128>() {
            if let Some(min) = min_amount {
                assert!(
                    amount_u128 >= min,
                    "Payment proposal amount should be >= min range"
                );
            }
            if let Some(max) = max_amount {
                assert!(
                    amount_u128 <= max,
                    "Payment proposal amount should be <= max range"
                );
            }
            if let Some(exact) = exact_amount {
                assert!(
                    amount_u128 == exact,
                    "Payment proposal should have exact amount"
                );
            }
        }
    }
}

// Helper function to validate response fields
fn verify_response_fields(response: &serde_json::Value, expected_fields: &[&str]) {
    for field in expected_fields {
        assert!(
            response.get(field).is_some(),
            "Response should have {} field",
            field
        );
    }
}

// Helper function to verify proposals are returned (for invalid/empty filters)
fn verify_proposals_returned(proposals: &[serde_json::Value], message: &str) {
    assert!(proposals.len() > 0, "{}", message);
}

// Helper function to verify sorting order
fn verify_sorting_order(proposals: &[serde_json::Value], ascending: bool, field: &str) {
    if proposals.len() > 1 {
        for i in 0..proposals.len() - 1 {
            let current_time = proposals[i]
                .get(field)
                .and_then(|t| t.as_str())
                .unwrap()
                .parse::<u64>()
                .unwrap();
            let next_time = proposals[i + 1]
                .get(field)
                .and_then(|t| t.as_str())
                .unwrap()
                .parse::<u64>()
                .unwrap();

            if ascending {
                assert!(
                    current_time <= next_time,
                    "Proposals should be sorted by {} ascending",
                    field
                );
            } else {
                assert!(
                    current_time >= next_time,
                    "Proposals should be sorted by {} descending",
                    field
                );
            }
        }
    }
}

// Helper function to verify all proposals have expected status
fn verify_proposal_status(proposals: &[serde_json::Value], expected_status: &str) {
    for proposal in proposals {
        let status = proposal.get("status").and_then(|s| s.as_str()).unwrap();
        assert_eq!(
            status, expected_status,
            "All proposals should have {} status",
            expected_status
        );
    }
}

// Helper function to verify all proposals contain expected keywords in description
fn verify_proposal_description_keywords(proposals: &[serde_json::Value], keywords: &[&str]) {
    for proposal in proposals {
        let description = proposal
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap();
        let description_lower = description.to_lowercase();
        let has_keyword = keywords
            .iter()
            .any(|keyword| description_lower.contains(keyword));
        assert!(
            has_keyword,
            "All proposals should contain at least one of the keywords: {:?}",
            keywords
        );
    }
}

// Helper function to verify all proposals have expected proposers (positive or negative)
fn verify_proposal_proposers(
    proposals: &[serde_json::Value],
    expected_proposers: &[&str],
    exclude: bool,
) {
    let expected_set: std::collections::HashSet<&str> =
        expected_proposers.iter().cloned().collect();
    for proposal in proposals {
        let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
        if exclude {
            assert!(
                !expected_set.contains(proposer),
                "All proposals should NOT have the excluded proposers: {:?}",
                expected_proposers
            );
        } else {
            assert!(
                expected_set.contains(proposer),
                "All proposals should have one of the specified proposers: {:?}",
                expected_proposers
            );
        }
    }
}

// Helper function to verify all proposals have votes from expected approvers (positive or negative)
fn verify_proposal_approvers(
    proposals: &[serde_json::Value],
    expected_approvers: &[&str],
    exclude: bool,
) {
    let expected_set: std::collections::HashSet<&str> =
        expected_approvers.iter().cloned().collect();
    for proposal in proposals {
        let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();
        let has_any_approver = votes.keys().any(|key| expected_set.contains(key.as_str()));
        if exclude {
            assert!(
                !has_any_approver,
                "All proposals should NOT have votes from the excluded approvers: {:?}",
                expected_approvers
            );
        } else {
            assert!(
                has_any_approver,
                "All proposals should have votes from at least one of the specified approvers: {:?}",
                expected_approvers
            );
        }
    }
}

// Helper function to verify all proposals are payment proposals with specific recipients (positive or negative)
fn verify_payment_recipients(
    proposals: &[serde_json::Value],
    expected_recipients: &[&str],
    exclude: bool,
) {
    let expected_set: std::collections::HashSet<&str> =
        expected_recipients.iter().cloned().collect();
    for proposal in proposals {
        if let Some(payment_info) = extract_payment_info(proposal) {
            if exclude {
                assert!(
                    !expected_set.contains(payment_info.receiver.as_str()),
                    "All payment proposals should NOT have the excluded recipients: {:?}",
                    expected_recipients
                );
            } else {
                assert!(
                    expected_set.contains(payment_info.receiver.as_str()),
                    "All payment proposals should have one of the specified recipients: {:?}",
                    expected_recipients
                );
            }
        } else {
            panic!("All proposals should be payment proposals");
        }
    }
}

// Helper function to verify all proposals are payment proposals with specific tokens (positive or negative)
fn verify_payment_tokens(proposals: &[serde_json::Value], expected_tokens: &[&str], exclude: bool) {
    let expected_set: std::collections::HashSet<&str> = expected_tokens.iter().cloned().collect();
    for proposal in proposals {
        if let Some(payment_info) = extract_payment_info(proposal) {
            let token = normalize_token(&payment_info.token);
            if exclude {
                assert!(
                    !expected_set.contains(token),
                    "All payment proposals should NOT have the excluded tokens: {:?}",
                    expected_tokens
                );
            } else {
                assert!(
                    expected_set.contains(token),
                    "All payment proposals should have one of the specified tokens: {:?}",
                    expected_tokens
                );
            }
        } else {
            panic!("All proposals should be payment proposals");
        }
    }
}

// Helper struct to extract stake delegation data from a proposal
#[derive(Debug)]
struct StakeDelegationData {
    proposal_type: String,
    validator: String,
    amount: u128,
}

// Helper function to extract stake delegation data from a proposal
fn extract_stake_delegation_data(proposal: &serde_json::Value) -> Option<StakeDelegationData> {
    let kind = proposal.get("kind")?;
    let function_call = kind.get("FunctionCall")?;
    let actions = function_call.get("actions")?.as_array()?;
    let action = actions.get(0)?;

    let method_name = action.get("method_name")?.as_str()?;
    let receiver_id = function_call.get("receiver_id")?.as_str()?;

    let proposal_type = match method_name {
        "deposit_and_stake" => "stake",
        "unstake" => "unstake",
        "withdraw_all" | "withdraw_all_from_staking_pool" => "withdraw",
        "select_staking_pool" => "whitelist",
        _ => "unknown",
    };

    // Extract amount from deposit field (for stake) or args (for unstake/withdraw)
    let mut amount = 0u128;

    // Check deposit amount for stake proposals
    if let Some(deposit_str) = action.get("deposit").and_then(|v| v.as_str()) {
        if let Ok(deposit_amount) = deposit_str.parse::<u128>() {
            amount = deposit_amount;
        }
    }

    // Check args for unstake/withdraw amounts
    if let Some(args_b64) = action.get("args").and_then(|a| a.as_str()) {
        if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(args_b64) {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&decoded_bytes) {
                if let Some(amount_from_args) = json.get("amount").and_then(|v| v.as_str()) {
                    if let Ok(args_amount) = amount_from_args.parse::<u128>() {
                        amount = args_amount;
                    }
                }
            }
        }
    }

    Some(StakeDelegationData {
        proposal_type: proposal_type.to_string(),
        validator: receiver_id.to_string(),
        amount,
    })
}

// Helper function to verify stake delegation proposals have expected types
fn verify_stake_delegation_types(proposals: &[serde_json::Value], expected_types: &[&str]) {
    let expected_set: std::collections::HashSet<&str> = expected_types.iter().cloned().collect();
    for proposal in proposals {
        if let Some(data) = extract_stake_delegation_data(proposal) {
            assert!(
                expected_set.contains(data.proposal_type.as_str()),
                "Proposal should have one of the expected stake types: {:?}, got: {}",
                expected_types,
                data.proposal_type
            );
        }
    }
}

// Helper function to verify stake delegation proposals have expected validators
fn verify_stake_delegation_validators(
    proposals: &[serde_json::Value],
    expected_validators: &[&str],
) {
    let expected_set: std::collections::HashSet<&str> =
        expected_validators.iter().cloned().collect();
    for proposal in proposals {
        if let Some(data) = extract_stake_delegation_data(proposal) {
            assert!(
                expected_set.contains(data.validator.as_str()),
                "Proposal should have one of the expected validators: {:?}, got: {}",
                expected_validators,
                data.validator
            );
        }
    }
}

// Helper function to verify stake delegation proposals have amounts within range
fn verify_stake_delegation_amounts(
    proposals: &[serde_json::Value],
    min_amount: Option<u128>,
    max_amount: Option<u128>,
) {
    for proposal in proposals {
        if let Some(data) = extract_stake_delegation_data(proposal) {
            if let Some(min) = min_amount {
                assert!(
                    data.amount >= min,
                    "Proposal amount {} should be >= minimum {}",
                    data.amount,
                    min
                );
            }

            if let Some(max) = max_amount {
                assert!(
                    data.amount <= max,
                    "Proposal amount {} should be <= maximum {}",
                    data.amount,
                    max
                );
            }
        }
    }
}

// Helper function to run a filter test
async fn run_filter_test<F>(client: &Client, test_name: &str, url: &str, verification_fn: F)
where
    F: FnOnce(&[serde_json::Value]),
{
    println!("Testing {}...", test_name);
    let response = make_request_and_parse(client, url).await;

    if let Some(proposals_array) = get_proposals_array(&response) {
        verification_fn(proposals_array);
    }
}

#[tokio::test]
async fn test_all_filters() {
    let client = get_test_client().await;

    // Test 1: Status filter
    run_filter_test(
        &client,
        "status filter",
        &format!("/proposals/{}?statuses=Approved", TEST_DAO_ID),
        |proposals| verify_proposal_status(proposals, "Approved"),
    )
    .await;

    // Test 2: Search filter
    run_filter_test(
        &client,
        "search filter",
        &format!("/proposals/{}?search=payment", TEST_DAO_ID),
        |proposals| verify_proposal_description_keywords(proposals, &["payment"]),
    )
    .await;

    // Test 3: Proposers filter
    run_filter_test(
        &client,
        "proposers filter",
        &format!(
            "/proposals/{}?proposers=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_proposal_proposers(proposals, &["megha19.near", "frol.near"], false),
    )
    .await;

    // Test 4: Proposers NOT filter
    run_filter_test(
        &client,
        "proposers NOT filter",
        &format!(
            "/proposals/{}?proposers_not=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_proposal_proposers(proposals, &["megha19.near", "frol.near"], true),
    )
    .await;

    // Test 5: Approvers filter
    run_filter_test(
        &client,
        "approvers filter",
        &format!(
            "/proposals/{}?approvers=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_proposal_approvers(proposals, &["megha19.near", "frol.near"], false),
    )
    .await;

    // Test 6: Approvers NOT filter
    run_filter_test(
        &client,
        "approvers NOT filter",
        &format!(
            "/proposals/{}?approvers_not=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_proposal_approvers(proposals, &["megha19.near", "frol.near"], true),
    )
    .await;

    // Test 7: Recipients filter
    run_filter_test(
        &client,
        "recipients filter",
        &format!(
            "/proposals/{}?category=payments&recipients=megha19.near",
            TEST_DAO_ID
        ),
        |proposals| verify_payment_recipients(proposals, &["megha19.near"], false),
    )
    .await;

    // Test 8: Recipients NOT filter
    run_filter_test(
        &client,
        "recipients NOT filter",
        &format!(
            "/proposals/{}?category=payments&recipients_not=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_payment_recipients(proposals, &["megha19.near", "frol.near"], true),
    )
    .await;

    // Test 9: Tokens filter
    run_filter_test(
        &client,
        "tokens filter",
        &format!("/proposals/{}?category=payments&tokens=near", TEST_DAO_ID),
        |proposals| verify_payment_tokens(proposals, &["near"], false),
    )
    .await;

    // Test 10: Tokens NOT filter
    run_filter_test(
        &client,
        "tokens NOT filter",
        &format!(
            "/proposals/{}?category=payments&tokens_not=near",
            TEST_DAO_ID
        ),
        |proposals| verify_payment_tokens(proposals, &["near"], true),
    )
    .await;

    // Test 11: Amount filters with specific token
    run_filter_test(
        &client,
        "amount min filter with NEAR token",
        &format!(
            "/proposals/{}?category=payments&tokens=near&amount_min=1.0",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                verify_payment_amount(proposal, Some(1000000000000000000000000), None, None);
            }
        },
    )
    .await;

    run_filter_test(
        &client,
        "amount max filter with NEAR token",
        &format!(
            "/proposals/{}?category=payments&tokens=near&amount_max=100.0",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                verify_payment_amount(proposal, None, Some(100000000000000000000000000), None);
            }
        },
    )
    .await;

    // Test 12: Pagination
    println!("Testing pagination...");
    let response = make_request_and_parse(
        &client,
        &format!("/proposals/{}?page=0&page_size=5", TEST_DAO_ID),
    )
    .await;

    // Verify pagination fields are present
    verify_response_fields(&response, &["page", "page_size", "total", "proposals"]);

    // Verify page_size is respected
    if let Some(proposals_array) = get_proposals_array(&response) {
        assert!(
            proposals_array.len() <= 5,
            "Number of proposals should not exceed page_size"
        );
    }

    // Test 13: Multiple filters
    run_filter_test(
        &client,
        "multiple filters",
        &format!(
            "/proposals/{}?statuses=Approved&category=payments&proposers=megha19.near&page_size=10",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_proposal_status(proposals, "Approved");

            // Check proposer
            for proposal in proposals {
                let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
                assert_eq!(
                    proposer, "megha19.near",
                    "All proposals should have the specified proposer"
                );

                // Check category (payment proposals)
                assert!(
                    extract_payment_info(proposal).is_some(),
                    "All proposals should be payment proposals"
                );
            }
        },
    )
    .await;

    // Test 14: Requested tokens endpoint
    println!("Testing requested tokens endpoint...");
    let response = make_request_and_parse(
        &client,
        &format!("/proposals/{}/requested-tokens", TEST_DAO_ID),
    )
    .await;

    // Verify response structure
    verify_response_fields(&response, &["requested_tokens", "total"]);

    // Verify tokens array
    if let Some(tokens_array) = response.get("requested_tokens").and_then(|t| t.as_array()) {
        for token in tokens_array {
            assert!(token.is_string(), "Each token should be a string");
        }
    }

    // Verify total count matches array length
    if let (Some(tokens_array), Some(total)) = (
        response.get("requested_tokens").and_then(|t| t.as_array()),
        response.get("total").and_then(|t| t.as_u64()),
    ) {
        assert_eq!(
            tokens_array.len() as u64,
            total,
            "Total count should match array length"
        );
    }

    // Test 15: Proposal types filter
    run_filter_test(
        &client,
        "proposal types filter",
        &format!(
            "/proposals/{}?proposal_types=FunctionCall,Transfer",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                let kind = proposal.get("kind").and_then(|k| k.as_object()).unwrap();
                let has_expected_type = kind.keys().any(|key| {
                    let key_str = key.as_str();
                    key_str == "FunctionCall" || key_str == "Transfer"
                });
                assert!(
                    has_expected_type,
                    "All proposals should have one of the specified proposal types"
                );
            }
        },
    )
    .await;

    // Test 16: Voter votes filter
    run_filter_test(
        &client,
        "voter votes filter",
        &format!(
            "/proposals/{}?voter_votes=megha19.near:approved",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();
                let megha_vote = votes.get("megha19.near").and_then(|v| v.as_str());
                assert!(
                    megha_vote == Some("Approve"),
                    "All proposals should have megha19.near voting Approve"
                );
            }
        },
    )
    .await;

    // Test 17: Amount equal filter
    run_filter_test(
        &client,
        "amount equal filter",
        &format!(
            "/proposals/{}?category=payments&tokens=near&amount_equal=1.5",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                verify_payment_amount(proposal, None, None, Some(1500000000000000000000000));
            }
        },
    )
    .await;

    // Test 18: Date range filters
    run_filter_test(
        &client,
        "date range filters",
        &format!(
            "/proposals/{}?created_date_from=2024-01-01&created_date_to=2024-12-31",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                let submission_time = proposal
                    .get("submission_time")
                    .and_then(|t| t.as_str())
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();

                // Convert dates to timestamps (nanoseconds)
                let from_timestamp = 1704067200000000000; // 2024-01-01 00:00:00 UTC
                let to_timestamp = 1735689599999999999; // 2024-12-31 23:59:59 UTC

                assert!(
                    submission_time >= from_timestamp && submission_time <= to_timestamp,
                    "All proposals should be within the specified date range"
                );
            }
        },
    )
    .await;

    // Test 19: Multiple voter votes filter
    run_filter_test(
        &client,
        "multiple voter votes filter",
        &format!(
            "/proposals/{}?voter_votes=megha19.near:approved,frol.near:rejected",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();

                // Check megha19.near voted Approve
                let megha_vote = votes.get("megha19.near").and_then(|v| v.as_str());
                assert!(
                    megha_vote == Some("Approve"),
                    "All proposals should have megha19.near voting Approve"
                );

                // Check frol.near voted Reject or Remove
                let frol_vote = votes.get("frol.near").and_then(|v| v.as_str());
                assert!(
                    frol_vote == Some("Reject") || frol_vote == Some("Remove"),
                    "All proposals should have frol.near voting Reject or Remove"
                );
            }
        },
    )
    .await;

    // Test 20: Human-readable amount filters
    run_filter_test(
        &client,
        "human-readable amount filters",
        &format!(
            "/proposals/{}?category=payments&tokens=near&amount_min=1.0&amount_max=10.0",
            TEST_DAO_ID
        ),
        |proposals| {
            for proposal in proposals {
                verify_payment_amount(
                    proposal,
                    Some(1000000000000000000000000),
                    Some(10000000000000000000000000),
                    None,
                );
            }
        },
    )
    .await;

    // Test 21: Proposers NOT filter
    run_filter_test(
        &client,
        "proposers NOT filter",
        &format!(
            "/proposals/{}?proposers_not=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_proposal_proposers(proposals, &["megha19.near", "frol.near"], true),
    )
    .await;

    // Test 22: Approvers NOT filter
    run_filter_test(
        &client,
        "approvers NOT filter",
        &format!(
            "/proposals/{}?approvers_not=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_proposal_approvers(proposals, &["megha19.near", "frol.near"], true),
    )
    .await;

    // Test 23: Recipients NOT filter
    run_filter_test(
        &client,
        "recipients NOT filter",
        &format!(
            "/proposals/{}?category=payments&recipients_not=megha19.near,frol.near",
            TEST_DAO_ID
        ),
        |proposals| verify_payment_recipients(proposals, &["megha19.near", "frol.near"], true),
    )
    .await;

    // Test 24: Tokens NOT filter
    run_filter_test(
        &client,
        "tokens NOT filter",
        &format!(
            "/proposals/{}?category=payments&tokens_not=near",
            TEST_DAO_ID
        ),
        |proposals| verify_payment_tokens(proposals, &["near"], true),
    )
    .await;

    // Test 25: Voter votes filter with non-existent voter
    run_filter_test(
        &client,
        "voter votes filter with non-existent voter",
        &format!(
            "/proposals/{}?voter_votes=nonexistent.near:approved",
            TEST_DAO_ID
        ),
        |proposals| {
            assert_eq!(
                proposals.len(),
                0,
                "No proposals should be returned when voter doesn't exist"
            );
        },
    )
    .await;

    // Test 26: Amount filters with invalid amounts
    run_filter_test(
        &client,
        "amount filters with invalid amounts",
        &format!(
            "/proposals/{}?category=payments&tokens=near&amount_min=invalid",
            TEST_DAO_ID
        ),
        |proposals| {
            assert_eq!(
                proposals.len(),
                0,
                "No proposals should be returned when amount is invalid"
            );
        },
    )
    .await;

    // Test 27: Date filters with invalid dates
    run_filter_test(
        &client,
        "date filters with invalid dates",
        &format!("/proposals/{}?created_date_from=invalid-date", TEST_DAO_ID),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "All proposals should be returned when date filter is invalid",
            );
        },
    )
    .await;

    // Test 28: Proposal types filter with non-existent types
    run_filter_test(
        &client,
        "proposal types filter with non-existent types",
        &format!("/proposals/{}?proposal_types=NonExistentType", TEST_DAO_ID),
        |proposals| {
            assert_eq!(
                proposals.len(),
                0,
                "No proposals should be returned when proposal type doesn't exist"
            );
        },
    )
    .await;

    // Test 30: Sorting filters
    run_filter_test(
        &client,
        "CreationTime ascending",
        &format!(
            "/proposals/{}?sort_by=CreationTime&sort_direction=asc",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_sorting_order(proposals, true, "submission_time");
        },
    )
    .await;

    run_filter_test(
        &client,
        "CreationTime descending",
        &format!(
            "/proposals/{}?sort_by=CreationTime&sort_direction=desc",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_sorting_order(proposals, false, "submission_time");
        },
    )
    .await;

    run_filter_test(
        &client,
        "ExpiryTime ascending",
        &format!(
            "/proposals/{}?sort_by=ExpiryTime&sort_direction=asc",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_sorting_order(proposals, true, "submission_time");
        },
    )
    .await;

    // Test 31: Multiple statuses filter
    run_filter_test(
        &client,
        "multiple statuses filter",
        &format!("/proposals/{}?statuses=Approved,Rejected", TEST_DAO_ID),
        |proposals| {
            for proposal in proposals {
                let status = proposal.get("status").and_then(|s| s.as_str()).unwrap();
                let expected_statuses = ["Approved", "Rejected"];
                assert!(
                    expected_statuses.contains(&status),
                    "All proposals should have one of the specified statuses"
                );
            }
        },
    )
    .await;

    // Test 32: Multiple search keywords
    run_filter_test(
        &client,
        "multiple search keywords",
        &format!("/proposals/{}?search=payment,budget", TEST_DAO_ID),
        |proposals| {
            verify_proposal_description_keywords(proposals, &["payment", "budget"]);
        },
    )
    .await;

    // Test 33: Empty filter values
    run_filter_test(
        &client,
        "empty statuses filter",
        &format!("/proposals/{}?statuses=", TEST_DAO_ID),
        |proposals| {
            assert!(
                proposals.is_empty(),
                "Empty statuses should return no proposals"
            );
        },
    )
    .await;

    run_filter_test(
        &client,
        "empty search filter",
        &format!("/proposals/{}?search=", TEST_DAO_ID),
        |proposals| {
            assert!(
                proposals.is_empty(),
                "Empty search should return no proposals"
            );
        },
    )
    .await;

    // Test 35: Invalid category
    run_filter_test(
        &client,
        "invalid category",
        &format!("/proposals/{}?category=invalid-category", TEST_DAO_ID),
        |proposals| {
            verify_proposals_returned(proposals, "Invalid category should return all proposals");
        },
    )
    .await;

    // Test 36: Invalid sort_by
    run_filter_test(
        &client,
        "invalid sort_by",
        &format!("/proposals/{}?sort_by=InvalidSort", TEST_DAO_ID),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "Invalid sort_by should return proposals without sorting",
            );
        },
    )
    .await;

    // Test 37: Stake delegation amount filter
    run_filter_test(
        &client,
        "stake delegation amount filter",
        &format!(
            "/proposals/{}?category=stake-delegation&stake_amount_min=1",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "Stake delegation amount filter should return proposals",
            );
            // Convert 1 NEAR to yocto NEAR (1 * 10^24)
            verify_stake_delegation_amounts(proposals, Some(1000000000000000000000000), None);
        },
    )
    .await;

    // Test 38: Stake delegation type filter
    run_filter_test(
        &client,
        "stake delegation type filter",
        &format!(
            "/proposals/{}?category=stake-delegation&stake_type=stake,unstake",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "Stake delegation type filter should return proposals",
            );
            verify_stake_delegation_types(proposals, &["stake", "unstake"]);
        },
    )
    .await;

    // Test 39: Stake delegation validator filter
    run_filter_test(
        &client,
        "stake delegation validator filter",
        &format!(
            "/proposals/{}?category=stake-delegation&validators=astro-stakers.poolv1.near",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "Stake delegation validator filter should return proposals",
            );
            verify_stake_delegation_validators(proposals, &["astro-stakers.poolv1.near"]);
        },
    )
    .await;

    // Test 40: Combined stake delegation filters
    run_filter_test(
        &client,
        "combined stake delegation filters",
        &format!(
            "/proposals/{}?category=stake-delegation&stake_type=stake&stake_amount_min=0.1&validators=astro-stakers.poolv1.near",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "Combined stake delegation filters should return proposals",
            );
            // Convert 0.1 NEAR to yocto NEAR (0.1 * 10^24)
            verify_stake_delegation_amounts(proposals, Some(100000000000000000000000), None);
            verify_stake_delegation_types(proposals, &["stake"]);
            verify_stake_delegation_validators(proposals, &["astro-stakers.poolv1.near"]);
        },
    )
    .await;

    // Test 41: Stake delegation amount range filter
    run_filter_test(
        &client,
        "stake delegation amount range filter",
        &format!(
            "/proposals/{}?category=stake-delegation&stake_amount_min=0.5&stake_amount_max=2.0",
            TEST_DAO_ID
        ),
        |proposals| {
            verify_proposals_returned(
                proposals,
                "Stake delegation amount range filter should return proposals",
            );
            // Convert 0.5 NEAR to yocto NEAR (0.5 * 10^24) and 2.0 NEAR to yocto NEAR (2.0 * 10^24)
            verify_stake_delegation_amounts(
                proposals,
                Some(500000000000000000000000),
                Some(2000000000000000000000000),
            );
        },
    )
    .await;

    println!("All filter tests completed successfully!");
}
