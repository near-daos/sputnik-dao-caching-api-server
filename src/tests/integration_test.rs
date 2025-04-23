#[cfg(test)]
mod test {
    use crate::rocket;
    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use serde_json::Value;

    #[test]
    fn test_get_dao_proposals() {
        // Set up the Rocket instance
        let rocket = rocket();

        // Create a client to send requests to the application
        let client = Client::tracked(rocket).expect("valid rocket instance");

        // Send a request to the endpoint
        // Note: In a real test, replace "sputnik-dao.near" with an actual DAO ID
        let response = client.get("/proposals/sputnik-dao.near").dispatch();

        // Verify the response status
        assert_eq!(response.status(), Status::Ok);

        // Parse and verify the response body
        let body_str = response.into_string().expect("response body");
        let body: Vec<Value> = serde_json::from_str(&body_str).expect("valid JSON");

        // In a real test, you'd add more specific assertions based on the expected response
        // For example:
        // assert!(!body.is_empty(), "Expected non-empty proposals list");
    }

    #[test]
    fn test_get_specific_proposal() {
        // Set up the Rocket instance
        let rocket = rocket();

        // Create a client to send requests to the application
        let client = Client::tracked(rocket).expect("valid rocket instance");

        // Send a request to the endpoint for a specific proposal
        // Note: In a real test, replace with actual DAO ID and proposal ID
        let response = client.get("/proposals/sputnik-dao.near/1").dispatch();

        // Verify the response status
        assert_eq!(response.status(), Status::Ok);

        // Parse and verify the response body
        let body_str = response.into_string().expect("response body");
        let proposal: Value = serde_json::from_str(&body_str).expect("valid JSON");

        // In a real test, add assertions to verify the proposal data
        // For example:
        // assert_eq!(proposal["proposal_id"], 1);
    }
}
