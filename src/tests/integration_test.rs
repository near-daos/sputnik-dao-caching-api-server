#[cfg(test)]
mod test {
    use crate::scraper::Proposal;
    use crate::{ProposalOutput, rocket};
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
    }
}
