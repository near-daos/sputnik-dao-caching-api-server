#[cfg(test)]
mod test {

    use csv::ReaderBuilder;
    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use sputnik_indexer::rocket;

    fn extract_csv_headers(csv_content: &str) -> Vec<String> {
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(csv_content.as_bytes());
        rdr.headers()
            .unwrap()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn csv_has_data_rows(csv_content: &str) -> bool {
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(csv_content.as_bytes());
        rdr.records().next().is_some()
    }

    #[test]
    fn test_csv_proposals_transfer_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?proposal_type=Transfer")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body = response.into_string().expect("response body");
        let headers = extract_csv_headers(&body);

        assert_eq!(
            headers,
            vec![
                "ID",
                "Created Date",
                "Status",
                "Title",
                "Summary",
                "Recipient",
                "Requested Token",
                "Funding Ask",
                "Created by",
                "Notes",
                "Approvers"
            ]
        );
        assert!(
            csv_has_data_rows(&body),
            "Should have at least one data row"
        );
    }

    #[test]
    fn test_csv_proposals_default_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body = response.into_string().expect("response body");
        let headers = extract_csv_headers(&body);

        assert_eq!(
            headers,
            vec!["ID", "Status", "Description", "Kind", "Approvers"]
        );
        assert!(
            csv_has_data_rows(&body),
            "Should have at least one data row"
        );
    }

    #[test]
    fn test_csv_proposals_stake_delegation_headers_and_row() {
        let client = Client::tracked(rocket()).expect("valid rocket instance");
        let response = client
            .get("/csv/proposals/testing-astradao.sputnik-dao.near?proposal_type=FunctionCall&keyword=stake")
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body = response.into_string().expect("response body");
        let headers = extract_csv_headers(&body);

        assert_eq!(
            headers,
            vec![
                "ID",
                "Status",
                "Type",
                "Amount",
                "Validator",
                "Created by",
                "Notes",
                "Approvers"
            ]
        );
        assert!(
            csv_has_data_rows(&body),
            "Should have at least one data row"
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
        let headers = extract_csv_headers(&body);

        assert_eq!(
            headers,
            vec![
                "ID",
                "Created At",
                "Status",
                "Recipient Account",
                "Amount",
                "Start Date",
                "End Date",
                "Cliff Date",
                "Allow Cancellation",
                "Allow Staking",
                "Approvers"
            ]
        );
        assert!(
            csv_has_data_rows(&body),
            "Should have at least one data row"
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
        let headers = extract_csv_headers(&body);

        assert_eq!(
            headers,
            vec![
                "ID",
                "Status",
                "Send Amount",
                "Send Token",
                "Receive Amount",
                "Receive Token",
                "Created By",
                "Notes",
                "Approvers"
            ]
        );
        assert!(
            csv_has_data_rows(&body),
            "Should have at least one data row"
        );
    }
}
