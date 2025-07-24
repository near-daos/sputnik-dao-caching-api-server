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
                            let decoded_bytes = decoded_bytes;
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

#[tokio::test]
async fn test_all_filters() {
    let client = get_test_client().await;

    // Test 1: Status filter
    println!("Testing status filter...");
    let response = make_request_and_parse(
        &client,
        &format!("/proposals/{}?statuses=Approved", TEST_DAO_ID),
    )
    .await;

    // Verify all returned proposals have Approved status
    if let Some(proposals_array) = get_proposals_array(&response) {
        for proposal in proposals_array {
            let status = proposal.get("status").and_then(|s| s.as_str()).unwrap();
            assert_eq!(
                status, "Approved",
                "All proposals should have Approved status"
            );
        }
    }

    // Test 2: Search filter
    println!("Testing search filter...");
    let response = client
        .get(format!("/proposals/{}?search=payment", TEST_DAO_ID))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals contain "payment" in description
    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            let description = proposal
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap();
            assert!(
                description.to_lowercase().contains("payment"),
                "All proposals should contain 'payment' in description"
            );
        }
    }

    // Test 3: Proposers filter
    println!("Testing proposers filter...");
    let response = client
        .get(format!(
            "/proposals/{}?proposers=megha19.near,frol.near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals have one of the specified proposers
    let expected_proposers: std::collections::HashSet<&str> =
        ["megha19.near", "frol.near"].iter().cloned().collect();

    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
            assert!(
                expected_proposers.contains(proposer),
                "All proposals should have one of the specified proposers"
            );
        }
    }

    // Test 4: Proposers NOT filter
    println!("Testing proposers NOT filter...");
    let response = client
        .get(format!(
            "/proposals/{}?proposers_not=megha19.near,frol.near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals do NOT have the excluded proposers
    let excluded_proposers: std::collections::HashSet<&str> =
        ["megha19.near", "frol.near"].iter().cloned().collect();

    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
            assert!(
                !excluded_proposers.contains(proposer),
                "All proposals should NOT have the excluded proposers"
            );
        }
    }

    // Test 5: Approvers filter
    println!("Testing approvers filter...");
    let response = client
        .get(format!(
            "/proposals/{}?approvers=megha19.near,frol.near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals have votes from at least one of the specified approvers
    let expected_approvers: std::collections::HashSet<&str> =
        ["megha19.near", "frol.near"].iter().cloned().collect();

    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();
            let has_any_approver = votes
                .keys()
                .any(|key| expected_approvers.contains(key.as_str()));
            assert!(
                has_any_approver,
                "All proposals should have votes from at least one of the specified approvers"
            );
        }
    }

    // Test 6: Approvers NOT filter
    println!("Testing approvers NOT filter...");
    let response = client
        .get(format!(
            "/proposals/{}?approvers_not=megha19.near,frol.near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals do NOT have votes from the excluded approvers
    let excluded_approvers: std::collections::HashSet<&str> =
        ["megha19.near", "frol.near"].iter().cloned().collect();

    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();
            let has_excluded_approver = votes
                .keys()
                .any(|key| excluded_approvers.contains(key.as_str()));
            assert!(
                !has_excluded_approver,
                "All proposals should NOT have votes from the excluded approvers"
            );
        }
    }

    // Test 7: Recipients filter
    println!("Testing recipients filter...");
    let response = client
        .get(format!(
            "/proposals/{}?category=payments&recipients=megha19.near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals have the specified recipient
    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            if let Some(payment_info) = extract_payment_info(proposal) {
                assert_eq!(
                    payment_info.receiver, "megha19.near",
                    "Payment proposal should have correct recipient"
                );
            }
        }
    }

    // Test 8: Recipients NOT filter
    println!("Testing recipients NOT filter...");
    let response = client
        .get(format!(
            "/proposals/{}?category=payments&recipients_not=megha19.near,frol.near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals do NOT have the excluded recipients
    if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
        for proposal in proposals_array {
            if let Some(payment_info) = extract_payment_info(proposal) {
                assert_ne!(
                    payment_info.receiver, "megha19.near",
                    "Payment proposal should NOT have excluded recipient"
                );
                assert_ne!(
                    payment_info.receiver, "frol.near",
                    "Payment proposal should NOT have excluded recipient"
                );
            }
        }
    }

    // Test 9: Tokens filter
    println!("Testing tokens filter...");
    let response = client
        .get(format!(
            "/proposals/{}?category=payments&tokens=near",
            TEST_DAO_ID
        ))
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);

    let response_body = response.into_string().await.unwrap();
    let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

    // Verify all returned proposals use the specified token
    if let Some(proposals_array) = get_proposals_array(&proposals) {
        for proposal in proposals_array {
            if let Some(payment_info) = extract_payment_info(proposal) {
                let token_to_check = normalize_token(&payment_info.token);
                assert_eq!(
                    token_to_check, "near",
                    "Payment proposal should have correct token"
                );
            }
        }
    }

    // Test 10: Tokens NOT filter
    println!("Testing tokens NOT filter...");
    let response = make_request_and_parse(
        &client,
        &format!(
            "/proposals/{}?category=payments&tokens_not=near",
            TEST_DAO_ID
        ),
    )
    .await;

    // Verify all returned proposals do NOT use the excluded token
    if let Some(proposals_array) = get_proposals_array(&response) {
        for proposal in proposals_array {
            if let Some(payment_info) = extract_payment_info(proposal) {
                let token_to_check = normalize_token(&payment_info.token);
                assert_ne!(
                    token_to_check, "near",
                    "Payment proposal should NOT have excluded token"
                );
            }
        }
    }

    // Test 11: Amount filters
    println!("Testing amount filters...");
    let response = make_request_and_parse(
        &client,
        &format!(
            "/proposals/{}?category=payments&amount_min=1000000000000000000000000",
            TEST_DAO_ID
        ),
    )
    .await;

    // Verify all returned proposals have amounts >= min
    if let Some(proposals_array) = get_proposals_array(&response) {
        for proposal in proposals_array {
            if let Some(payment_info) = extract_payment_info(proposal) {
                if let Ok(amount_u128) = payment_info.amount.parse::<u128>() {
                    assert!(
                        amount_u128 >= 1000000000000000000000000,
                        "Payment proposal amount should be >= min range"
                    );
                }
            }
        }
    }

    // Test amount_max filter
    let response = make_request_and_parse(
        &client,
        &format!(
            "/proposals/{}?category=payments&amount_max=10000000000000000000000000",
            TEST_DAO_ID
        ),
    )
    .await;

    // Verify all returned proposals have amounts <= max
    if let Some(proposals_array) = get_proposals_array(&response) {
        for proposal in proposals_array {
            if let Some(payment_info) = extract_payment_info(proposal) {
                if let Ok(amount_u128) = payment_info.amount.parse::<u128>() {
                    assert!(
                        amount_u128 <= 10000000000000000000000000,
                        "Payment proposal amount should be <= max range"
                    );
                }
            }
        }
    }

    // Test 12: Pagination
    println!("Testing pagination...");
    let response = make_request_and_parse(
        &client,
        &format!("/proposals/{}?page=0&page_size=5", TEST_DAO_ID),
    )
    .await;

    // Verify pagination fields are present
    assert!(
        response.get("page").is_some(),
        "Response should have page field"
    );
    assert!(
        response.get("page_size").is_some(),
        "Response should have page_size field"
    );
    assert!(
        response.get("total").is_some(),
        "Response should have total field"
    );
    assert!(
        response.get("proposals").is_some(),
        "Response should have proposals field"
    );

    // Verify page_size is respected
    if let Some(proposals_array) = get_proposals_array(&response) {
        assert!(
            proposals_array.len() <= 5,
            "Number of proposals should not exceed page_size"
        );
    }

    // Test 13: Multiple filters
    println!("Testing multiple filters...");
    let response = make_request_and_parse(
        &client,
        &format!(
            "/proposals/{}?statuses=Approved&category=payments&proposers=megha19.near&page_size=10",
            TEST_DAO_ID
        ),
    )
    .await;

    // Verify all returned proposals meet all filter criteria
    if let Some(proposals_array) = get_proposals_array(&response) {
        for proposal in proposals_array {
            // Check status
            let status = proposal.get("status").and_then(|s| s.as_str()).unwrap();
            assert_eq!(
                status, "Approved",
                "All proposals should have Approved status"
            );

            // Check proposer
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
    }

    // Test 14: Requested tokens endpoint
    println!("Testing requested tokens endpoint...");
    let response = make_request_and_parse(
        &client,
        &format!("/proposals/{}/requested-tokens", TEST_DAO_ID),
    )
    .await;

    // Verify response structure
    assert!(
        response.get("requested_tokens").is_some(),
        "Response should have requested_tokens field"
    );
    assert!(
        response.get("total").is_some(),
        "Response should have total field"
    );

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

    println!("All filter tests completed successfully!");
}

// #[tokio::test]
// async fn test_proposers_filter() {
//     let client = get_test_client().await;

//     // Test proposers filter
//     let response = client
//         .get(format!(
//             "/proposals/{}?proposers=megha19.near,frol.near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals have one of the specified proposers
//     let expected_proposers: HashSet<&str> = ["megha19.near", "frol.near"].iter().cloned().collect();

//     if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
//         for proposal in proposals_array {
//             let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
//             assert!(
//                 expected_proposers.contains(proposer),
//                 "All proposals should have one of the specified proposers"
//             );
//         }
//     }
// }

// #[tokio::test]
// async fn test_proposers_not_filter() {
//     let client = get_test_client().await;

//     // Test proposers_not filter
//     let response = client
//         .get(format!(
//             "/proposals/{}?proposers_not=megha19.near,frol.near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals do NOT have the excluded proposers
//     let excluded_proposers: HashSet<&str> = ["megha19.near", "frol.near"].iter().cloned().collect();

//     if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
//         for proposal in proposals_array {
//             let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
//             assert!(
//                 !excluded_proposers.contains(proposer),
//                 "All proposals should NOT have the excluded proposers"
//             );
//         }
//     }
// }

// #[tokio::test]
// async fn test_approvers_filter() {
//     let client = get_test_client().await;

//     // Test approvers filter
//     let response = client
//         .get(format!(
//             "/proposals/{}?approvers=megha19.near,frol.near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals have votes from at least one of the specified approvers
//     let expected_approvers: HashSet<&str> = ["megha19.near", "frol.near"].iter().cloned().collect();

//     if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
//         for proposal in proposals_array {
//             let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();
//             let has_any_approver = votes
//                 .keys()
//                 .any(|key| expected_approvers.contains(key.as_str()));
//             assert!(
//                 has_any_approver,
//                 "All proposals should have votes from at least one of the specified approvers"
//             );
//         }
//     }
// }

// #[tokio::test]
// async fn test_approvers_not_filter() {
//     let client = get_test_client().await;

//     // Test approvers_not filter
//     let response = client
//         .get(format!(
//             "/proposals/{}?approvers_not=megha19.near,frol.near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals do NOT have votes from the excluded approvers
//     let excluded_approvers: HashSet<&str> = ["megha19.near", "frol.near"].iter().cloned().collect();

//     if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
//         for proposal in proposals_array {
//             let votes = proposal.get("votes").and_then(|v| v.as_object()).unwrap();
//             let has_excluded_approver = votes
//                 .keys()
//                 .any(|key| excluded_approvers.contains(key.as_str()));
//             assert!(
//                 !has_excluded_approver,
//                 "All proposals should NOT have votes from the excluded approvers"
//             );
//         }
//     }
// }

// #[tokio::test]
// async fn test_recipients_filter() {
//     let client = get_test_client().await;

//     // Test recipients filter for payments
//     let response = client
//         .get(format!(
//             "/proposals/{}?category=payments&recipients=megha19.near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals have the specified recipient
//     if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
//         for proposal in proposals_array {
//             if let Some(payment_info) = extract_payment_info(proposal) {
//                 assert_eq!(
//                     payment_info.receiver, "megha19.near",
//                     "Payment proposal should have correct recipient"
//                 );
//             }
//         }
//     }
// }

// #[tokio::test]
// async fn test_recipients_not_filter() {
//     let client = get_test_client().await;

//     // Test recipients_not filter for payments
//     let response = client
//         .get(format!(
//             "/proposals/{}?category=payments&recipients_not=megha19.near,frol.near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals do NOT have the excluded recipients
//     if let Some(proposals_array) = proposals.get("proposals").and_then(|p| p.as_array()) {
//         for proposal in proposals_array {
//             if let Some(payment_info) = extract_payment_info(proposal) {
//                 assert_ne!(
//                     payment_info.receiver, "megha19.near",
//                     "Payment proposal should NOT have excluded recipient"
//                 );
//                 assert_ne!(
//                     payment_info.receiver, "frol.near",
//                     "Payment proposal should NOT have excluded recipient"
//                 );
//             }
//         }
//     }
// }

// #[tokio::test]
// async fn test_tokens_filter() {
//     let client = get_test_client().await;

//     // Test tokens filter for payments
//     let response = client
//         .get(format!(
//             "/proposals/{}?category=payments&tokens=near",
//             TEST_DAO_ID
//         ))
//         .dispatch()
//         .await;

//     assert_eq!(response.status(), Status::Ok);

//     let response_body = response.into_string().await.unwrap();
//     let proposals: serde_json::Value = serde_json::from_str(&response_body).unwrap();

//     // Verify all returned proposals use the specified token
//     if let Some(proposals_array) = get_proposals_array(&proposals) {
//         for proposal in proposals_array {
//             if let Some(payment_info) = extract_payment_info(proposal) {
//                 let token_to_check = normalize_token(&payment_info.token);
//                 assert_eq!(
//                     token_to_check, "near",
//                     "Payment proposal should have correct token"
//                 );
//             }
//         }
//     }
// }

// #[tokio::test]
// async fn test_tokens_not_filter() {
//     let client = get_test_client().await;

//     // Test tokens_not filter for payments
//     let response = make_request_and_parse(
//         &client,
//         &format!(
//             "/proposals/{}?category=payments&tokens_not=near",
//             TEST_DAO_ID
//         ),
//     )
//     .await;

//     // Verify all returned proposals do NOT use the excluded token
//     if let Some(proposals_array) = get_proposals_array(&response) {
//         for proposal in proposals_array {
//             if let Some(payment_info) = extract_payment_info(proposal) {
//                 let token_to_check = normalize_token(&payment_info.token);
//                 assert_ne!(
//                     token_to_check, "near",
//                     "Payment proposal should NOT have excluded token"
//                 );
//             }
//         }
//     }
// }

// #[tokio::test]
// async fn test_amount_filters() {
//     let client = get_test_client().await;

//     // Test amount_min filter
//     let response = make_request_and_parse(
//         &client,
//         &format!(
//             "/proposals/{}?category=payments&amount_min=1000000000000000000000000",
//             TEST_DAO_ID
//         ),
//     )
//     .await;

//     // Verify all returned proposals have amounts >= min
//     if let Some(proposals_array) = get_proposals_array(&response) {
//         for proposal in proposals_array {
//             if let Some(payment_info) = extract_payment_info(proposal) {
//                 if let Ok(amount_u128) = payment_info.amount.parse::<u128>() {
//                     assert!(
//                         amount_u128 >= 1000000000000000000000000,
//                         "Payment proposal amount should be >= min range"
//                     );
//                 }
//             }
//         }
//     }

//     // Test amount_max filter
//     let response = make_request_and_parse(
//         &client,
//         &format!(
//             "/proposals/{}?category=payments&amount_max=10000000000000000000000000",
//             TEST_DAO_ID
//         ),
//     )
//     .await;

//     // Verify all returned proposals have amounts <= max
//     if let Some(proposals_array) = get_proposals_array(&response) {
//         for proposal in proposals_array {
//             if let Some(payment_info) = extract_payment_info(proposal) {
//                 if let Ok(amount_u128) = payment_info.amount.parse::<u128>() {
//                     assert!(
//                         amount_u128 <= 10000000000000000000000000,
//                         "Payment proposal amount should be <= max range"
//                     );
//                 }
//             }
//         }
//     }
// }

// #[tokio::test]
// async fn test_pagination() {
//     let client = get_test_client().await;

//     // Test pagination
//     let response = make_request_and_parse(
//         &client,
//         &format!("/proposals/{}?page=0&page_size=5", TEST_DAO_ID),
//     )
//     .await;

//     // Verify pagination fields are present
//     assert!(
//         response.get("page").is_some(),
//         "Response should have page field"
//     );
//     assert!(
//         response.get("page_size").is_some(),
//         "Response should have page_size field"
//     );
//     assert!(
//         response.get("total").is_some(),
//         "Response should have total field"
//     );
//     assert!(
//         response.get("proposals").is_some(),
//         "Response should have proposals field"
//     );

//     // Verify page_size is respected
//     if let Some(proposals_array) = get_proposals_array(&response) {
//         assert!(
//             proposals_array.len() <= 5,
//             "Number of proposals should not exceed page_size"
//         );
//     }
// }

// #[tokio::test]
// async fn test_multiple_filters() {
//     let client = get_test_client().await;

//     // Test multiple filters together
//     let response = make_request_and_parse(
//         &client,
//         &format!(
//             "/proposals/{}?statuses=Approved&category=payments&proposers=megha19.near&page_size=10",
//             TEST_DAO_ID
//         ),
//     )
//     .await;

//     // Verify all returned proposals meet all filter criteria
//     if let Some(proposals_array) = get_proposals_array(&response) {
//         for proposal in proposals_array {
//             // Check status
//             let status = proposal.get("status").and_then(|s| s.as_str()).unwrap();
//             assert_eq!(
//                 status, "Approved",
//                 "All proposals should have Approved status"
//             );

//             // Check proposer
//             let proposer = proposal.get("proposer").and_then(|p| p.as_str()).unwrap();
//             assert_eq!(
//                 proposer, "megha19.near",
//                 "All proposals should have the specified proposer"
//             );

//             // Check category (payment proposals)
//             assert!(
//                 extract_payment_info(proposal).is_some(),
//                 "All proposals should be payment proposals"
//             );
//         }
//     }
// }

// #[tokio::test]
// async fn test_requested_tokens_endpoint() {
//     let client = get_test_client().await;

//     // Test requested tokens endpoint
//     let response = make_request_and_parse(
//         &client,
//         &format!("/proposals/{}/requested-tokens", TEST_DAO_ID),
//     )
//     .await;

//     // Verify response structure
//     assert!(
//         response.get("requested_tokens").is_some(),
//         "Response should have requested_tokens field"
//     );
//     assert!(
//         response.get("total").is_some(),
//         "Response should have total field"
//     );

//     // Verify tokens array
//     if let Some(tokens_array) = response.get("requested_tokens").and_then(|t| t.as_array()) {
//         for token in tokens_array {
//             assert!(token.is_string(), "Each token should be a string");
//         }
//     }

//     // Verify total count matches array length
//     if let (Some(tokens_array), Some(total)) = (
//         response.get("requested_tokens").and_then(|t| t.as_array()),
//         response.get("total").and_then(|t| t.as_u64()),
//     ) {
//         assert_eq!(
//             tokens_array.len() as u64,
//             total,
//             "Total count should match array length"
//         );
//     }
// }
