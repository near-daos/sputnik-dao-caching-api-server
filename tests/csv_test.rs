#[cfg(test)]
mod test {

    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use sputnik_indexer::rocket;

    #[test]
    fn test_all_csv_proposals_with_shared_cache() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");

        // Test 1: Default headers and row
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().expect("response body");
        let lines: Vec<&str> = body.lines().collect();
        let expected_headers = "ID,Created Date,Status,Description,Kind,Created by,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");

        // Test 2: Stake delegation
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?category=stake-delegation")
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

        // Test 3: Lockup
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?category=lockup")
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

        // Test 4: Asset exchange
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?category=asset-exchange")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().expect("response body");
        let lines: Vec<&str> = body.lines().collect();
        let expected_headers = "ID,Created Date,Status,Send Amount,Send Token,Receive Amount,Receive Token,Created By,Notes,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");
        let expected_first_row = "193,2025-02-28 12:38:54 UTC,Approved,0.1,USDC,0.10007,USDt,megha19.near,,megha19.near,";
        assert_eq!(
            lines[1], expected_first_row,
            "First data row does not match"
        );

        // Test 5: Payments
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?category=payments")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().expect("response body");
        let lines: Vec<&str> = body.lines().collect();
        let expected_headers = "ID,Created Date,Status,Title,Summary,Recipient,Requested Token,Funding Ask,Created by,Notes,Approvers (Approved),Approvers (Rejected/Remove)";
        assert_eq!(lines[0], expected_headers, "Headers do not match");
        let expected_first_row = "15,2024-08-06 19:34:18 UTC,Rejected,DevHub Activities Report 7/22-8/4,DevHub Moderator Contributions Bi-Weekly Report,joespano.near,USDC,1.00000,megha19.near,this is notes,,\"megha19.near, theori.near\"";
        assert_eq!(
            lines[1], expected_first_row,
            "First data row does not match"
        );
    }
}
