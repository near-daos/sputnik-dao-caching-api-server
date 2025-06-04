#[cfg(test)]
mod test {

    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use sputnik_indexer::rocket;

    #[test]
    fn test_csv_proposals_default_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body = response.into_string().expect("response body");

        let lines: Vec<&str> = body.lines().collect();

        let expected_headers = "ID,Created Date,Status,Description,Kind,Created by,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");
    }

    #[test]
    fn test_csv_proposals_stake_delegation_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?proposal_type=FunctionCall&keyword=stake")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body = response.into_string().expect("response body");

        let lines: Vec<&str> = body.lines().collect();

        let expected_headers = "ID,Created Date,Status,Type,Amount,Token,Validator,Created by,Notes,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");

        let expected_first_row = "70,2024-10-04 09:10:17 UTC,Approved,Stake,1.00000,NEAR,astro-stakers.poolv1.near,megha19.near,Testing Stake,megha19.near,";
        assert_eq!(
            lines[1], expected_first_row,
            "First data row does not match"
        );
    }

    #[test]
    fn test_csv_proposals_lockup_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?proposal_type=FunctionCall&keyword=lockup")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body = response.into_string().expect("response body");

        let lines: Vec<&str> = body.lines().collect();

        let expected_headers = "ID,Created Date,Status,Recipient Account,Amount,Token,Start Date,End Date,Cliff Date,Allow Cancellation,Allow Staking,Created by,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");

        let expected_first_row = "197,2025-03-04 19:24:53 UTC,Approved,rubycop.near,4.00000,NEAR,1970-01-21 03:40:19 UTC,1970-01-21 03:41:45 UTC,1970-01-21 03:40:19 UTC,yes,yes,rubycop.near,rubycop.near,";
        assert_eq!(
            lines[1], expected_first_row,
            "First data row does not match"
        );
    }

    #[test]
    fn test_csv_proposals_asset_exchange_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?proposal_type=FunctionCall&keyword=asset")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().expect("response body");

        let lines: Vec<&str> = body.lines().collect();

        let expected_headers = "ID,Created Date,Status,Send Amount,Send Token,Receive Amount,Receive Token,Created By,Notes,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");

        let expected_first_row =
            "65,2024-09-17 10:50:30 UTC,Expired,1,USDC,0.99990,USDt,megha19.near,null,,";
        assert_eq!(
            lines[1], expected_first_row,
            "First data row does not match"
        );
    }

    #[test]
    fn test_csv_proposals_transfer_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?proposal_type=Transfer")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().expect("response body");

        let lines: Vec<&str> = body.lines().collect();

        let expected_headers = "ID,Created Date,Status,Title,Summary,Recipient,Requested Token,Funding Ask,Created by,Notes,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");

        let expected_first_row = "15,2024-08-06 19:34:18 UTC,Rejected,DevHub Activities Report 7/22-8/4,DevHub Moderator Contributions Bi-Weekly Report,joespano.near,,1000000.00000,megha19.near,this is notes,,\"megha19.near, theori.near\"";
        assert_eq!(
            lines[1], expected_first_row,
            "First data row does not match"
        );
    }
}
