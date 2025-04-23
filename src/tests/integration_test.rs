#[cfg(test)]
mod test {
    use crate::rocket;
    use rocket::http::Status;
    use rocket::local::blocking::Client;
    use serde_json::Value;

    #[test]
    fn test_get_dao_proposals() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        let response = client.get("/proposals/account-0.test.near").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body_str = response.into_string().expect("response body");
        let body: Vec<Value> = serde_json::from_str(&body_str).expect("valid JSON");
    }

    #[test]
    fn test_get_specific_proposal() {
        let rocket = rocket();
        let client = Client::tracked(rocket).expect("valid rocket instance");

        let response = client.get("/proposals/account-0.test.near/0").dispatch();
        assert_eq!(response.status(), Status::Ok);

        let body_str = response.into_string().expect("response body");
        let proposal: Value = serde_json::from_str(&body_str).expect("valid JSON");
    }
}
