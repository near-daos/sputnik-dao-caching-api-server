#[cfg(test)]
mod test {

    use crate::scraper::Proposal;
    use crate::{ProposalOutput, rocket};
    use near_sdk::NearToken;
    use rocket::http::Status;
    use rocket::local::blocking::Client;

    #[test]
    fn test_get_dao_proposals() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        let response = client.get("/proposals/account-0.test.near").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body_str = response.into_string().expect("response body");
        let proposals: Vec<Proposal> = serde_json::from_str(&body_str).expect("valid JSON");
        assert_eq!(proposals.len(), 2);
    }

    #[test]
    fn test_get_specific_proposal() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        let response = client.get("/proposals/account-0.test.near/0").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body_str = response.into_string().expect("response body");
        let proposal: ProposalOutput = serde_json::from_str(&body_str).expect("valid JSON");
        // 25 votes were made + 1 tx of proposal creation.
        assert_eq!(proposal.txs_log.len(), 26);

        // Verify txs_log is in chronological order (ascending) by timestamp
        for i in 1..proposal.txs_log.len() {
            assert!(
                proposal.txs_log[i - 1].timestamp <= proposal.txs_log[i].timestamp,
                "Transaction logs should be in chronological order by timestamp"
            );
        }
    }

    #[test]
    fn test_propose_kind_filtering() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        // Filter by transfer receiver
        let type_response = client
            .get("/proposals/account-0.test.near?proposal_type=Transfer:receiver_id")
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 2, "Both proposals should be found.");

        let type_response = client
            .get("/proposals/account-0.test.near?proposal_type=Transfer:receiver_id:=account-2.test.near")
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 2, "Both proposals should be found.");

        let type_response = client
            .get("/proposals/account-0.test.near?proposal_type=Transfer:receiver_id:=account-0.test.near")
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 0, "No proposals should be found.");

        // Filter by amount
        let type_response = client
            .get(format!(
                "/proposals/account-0.test.near?proposal_type=Transfer:amount:={}",
                NearToken::from_millinear(10).as_yoctonear() // 0.01 Near
            ))
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 2, "Both proposals should be found.");

        let type_response = client
            .get(format!(
                "/proposals/account-0.test.near?proposal_type=Transfer:amount:%3E{}", // >
                NearToken::from_millinear(11).as_yoctonear()                          // 0.011 Near
            ))
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 2, "Both proposals should be found.");

        let type_response = client
            .get(format!(
                "/proposals/account-0.test.near?proposal_type=Transfer:amount:%3C{}", // <
                NearToken::from_millinear(11).as_yoctonear()                          // 0.011 Near
            ))
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 0, "No proposals should be found.");
    }

    #[test]
    fn test_all_filter_options() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        // First get all proposals - should be exactly 2
        let all_response = client.get("/proposals/account-0.test.near").dispatch();
        assert_eq!(all_response.status(), Status::Ok);
        let all_body = all_response.into_string().expect("response body");
        let all_proposals: Vec<Proposal> = serde_json::from_str(&all_body).expect("valid JSON");
        assert_eq!(
            all_proposals.len(),
            2,
            "There should be exactly 2 proposals"
        );

        // Test proposal_type filter - both should be transfers
        let type_response = client
            .get("/proposals/account-0.test.near?proposal_type=Transfer")
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(
            type_proposals.len(),
            2,
            "Both proposals should be transfers"
        );
        for proposal in &type_proposals {
            assert!(
                proposal.kind.get("Transfer").is_some(),
                "Proposal kind should be Transfer"
            );
        }

        // Test another proposal type
        let type_response = client
            .get("/proposals/account-0.test.near?proposal_type=Bounty&proposal_type=Transfer")
            .dispatch();
        assert_eq!(type_response.status(), Status::Ok);
        let type_body = type_response.into_string().expect("response body");
        let type_proposals: Vec<Proposal> = serde_json::from_str(&type_body).expect("valid JSON");
        assert_eq!(type_proposals.len(), 0, "No proposals should be found.");

        // Test min_votes filter with min_votes=1 - should return only the one with votes
        let min_votes_response = client
            .get("/proposals/account-0.test.near?min_votes=1")
            .dispatch();
        assert_eq!(min_votes_response.status(), Status::Ok);
        let min_votes_body = min_votes_response.into_string().expect("response body");
        let min_votes_proposals: Vec<Proposal> =
            serde_json::from_str(&min_votes_body).expect("valid JSON");
        assert_eq!(
            min_votes_proposals.len(),
            1,
            "Only one proposal should have votes"
        );
        assert_eq!(
            min_votes_proposals[0].votes.len(),
            25,
            "This proposal should have exactly 25 votes"
        );

        // Test min_votes filter with min_votes=0 - should return both
        let zero_votes_response = client
            .get("/proposals/account-0.test.near?min_votes=0")
            .dispatch();
        assert_eq!(zero_votes_response.status(), Status::Ok);
        let zero_votes_body = zero_votes_response.into_string().expect("response body");
        let zero_votes_proposals: Vec<Proposal> =
            serde_json::from_str(&zero_votes_body).expect("valid JSON");
        assert_eq!(
            zero_votes_proposals.len(),
            2,
            "Both proposals should match min_votes=0"
        );

        // Test combined filters - Transfer type with votes
        let combined_response = client
            .get("/proposals/account-0.test.near?proposal_type=Transfer&min_votes=1")
            .dispatch();
        assert_eq!(combined_response.status(), Status::Ok);
        let combined_body = combined_response.into_string().expect("response body");
        let combined_proposals: Vec<Proposal> =
            serde_json::from_str(&combined_body).expect("valid JSON");
        assert_eq!(
            combined_proposals.len(),
            1,
            "Only one proposal should match both filters"
        );
        assert_eq!(
            combined_proposals[0].votes.len(),
            25,
            "This proposal should have exactly 25 votes"
        );
        assert!(
            combined_proposals[0].kind.get("Transfer").is_some(),
            "This proposal should be a Transfer"
        );
    }

    #[test]
    fn test_sorting_proposals() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        // Test ascending sort by creation time
        let asc_response = client
            .get("/proposals/account-0.test.near?sort_by=CreationTime&sort_direction=asc")
            .dispatch();
        assert_eq!(asc_response.status(), Status::Ok);

        let asc_body = asc_response.into_string().expect("response body");
        let asc_proposals: Vec<Proposal> = serde_json::from_str(&asc_body).expect("valid JSON");

        // Verify proposals are in ascending order by submission_time
        for i in 1..asc_proposals.len() {
            assert!(
                asc_proposals[i - 1].submission_time <= asc_proposals[i].submission_time,
                "Proposals should be in ascending order by submission_time"
            );
        }

        // Test descending sort by creation time
        let desc_response = client
            .get("/proposals/account-0.test.near?sort_by=CreationTime&sort_direction=desc")
            .dispatch();
        assert_eq!(desc_response.status(), Status::Ok);

        let desc_body = desc_response.into_string().expect("response body");
        let desc_proposals: Vec<Proposal> = serde_json::from_str(&desc_body).expect("valid JSON");

        // Verify proposals are in descending order by submission_time
        for i in 1..desc_proposals.len() {
            assert!(
                desc_proposals[i - 1].submission_time >= desc_proposals[i].submission_time,
                "Proposals should be in descending order by submission_time"
            );
        }
    }
}
